use std::collections::BTreeMap;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{EdgeKind, Method};
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects wait/await calls outside backward-loop regions.
#[derive(Default)]
pub(crate) struct WaitNotGuardedByLoopRule;

crate::register_rule!(WaitNotGuardedByLoopRule);

impl Rule for WaitNotGuardedByLoopRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "WAIT_NOT_GUARDED_BY_LOOP",
            name: "Wait call not guarded by loop",
            description: "wait/await calls outside retry loops risk spurious wakeups",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("scan.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    let artifact_uri = context.class_artifact_uri(class);
                    for method in &class.methods {
                        let loop_ranges = loop_ranges(method);
                        for call in &method.calls {
                            if !is_wait_like_call(&call.owner, &call.name, &call.descriptor) {
                                continue;
                            }
                            if is_guarded_by_loop(&loop_ranges, call.offset) {
                                continue;
                            }
                            let message = result_message(format!(
                                "Wrap wait/await in a condition-checking loop in {}.{}{}; re-check the condition after wakeup to handle spurious wakeups.",
                                class.name, method.name, method.descriptor
                            ));
                            let line = method.line_for_offset(call.offset);
                            let location = method_location_with_line(
                                &class.name,
                                &method.name,
                                &method.descriptor,
                                artifact_uri.as_deref(),
                                line,
                            );
                            class_results.push(
                                SarifResult::builder()
                                    .message(message)
                                    .locations(vec![location])
                                    .build(),
                            );
                        }
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn is_wait_like_call(owner: &str, name: &str, descriptor: &str) -> bool {
    if owner == "java/lang/Object" && name == "wait" {
        return matches!(descriptor, "()V" | "(J)V" | "(JI)V");
    }

    let condition_owner = owner == "java/util/concurrent/locks/Condition"
        || owner == "java/util/concurrent/locks/AbstractQueuedSynchronizer$ConditionObject";
    if !condition_owner {
        return false;
    }

    matches!(
        (name, descriptor),
        ("await", "()V")
            | ("awaitUninterruptibly", "()V")
            | ("awaitNanos", "(J)J")
            | ("awaitUntil", "(Ljava/util/Date;)Z")
            | ("await", "(JLjava/util/concurrent/TimeUnit;)Z")
    )
}

fn loop_ranges(method: &Method) -> Vec<(u32, u32)> {
    let block_end_offsets = method
        .cfg
        .blocks
        .iter()
        .map(|block| (block.start_offset, block.end_offset))
        .collect::<BTreeMap<_, _>>();

    let mut ranges = Vec::new();
    for edge in &method.cfg.edges {
        if edge.kind != EdgeKind::Branch || edge.from <= edge.to {
            continue;
        }
        let Some(loop_end_offset) = block_end_offsets.get(&edge.from) else {
            continue;
        };
        ranges.push((edge.to, *loop_end_offset));
    }
    ranges.sort_unstable();
    ranges.dedup();
    ranges
}

fn is_guarded_by_loop(loop_ranges: &[(u32, u32)], call_offset: u32) -> bool {
    loop_ranges
        .iter()
        .any(|(start_offset, end_offset)| *start_offset <= call_offset && call_offset < *end_offset)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn wait_loop_messages(output: &crate::engine::EngineOutput) -> Vec<String> {
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("WAIT_NOT_GUARDED_BY_LOOP"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn compile_and_analyze(
        harness: &JvmTestHarness,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> crate::engine::EngineOutput {
        harness
            .compile_and_analyze(Language::Java, sources, classpath)
            .expect("run harness analysis")
    }

    #[test]
    fn wait_not_guarded_by_loop_reports_wait_under_if() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
public class ClassA {
    public void methodOne(Object varOne, boolean varTwo) throws Exception {
        synchronized (varOne) {
            if (!varTwo) {
                varOne.wait();
            }
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_loop_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("condition-checking loop")),
            "expected WAIT_NOT_GUARDED_BY_LOOP finding, got {messages:?}"
        );
    }

    #[test]
    fn wait_not_guarded_by_loop_ignores_wait_inside_while() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;
public class ClassB {
    public void methodTwo(Object varOne, boolean varTwo) throws Exception {
        synchronized (varOne) {
            while (!varTwo) {
                varOne.wait();
                varTwo = true;
            }
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_loop_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect WAIT_NOT_GUARDED_BY_LOOP finding for while-guarded wait: {messages:?}"
        );
    }

    #[test]
    fn wait_not_guarded_by_loop_reports_condition_await_under_if() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;
public class ClassC {
    public void methodThree(boolean varOne) throws Exception {
        ReentrantLock varTwo = new ReentrantLock();
        Condition varThree = varTwo.newCondition();
        varTwo.lock();
        try {
            if (!varOne) {
                varThree.await();
            }
        } finally {
            varTwo.unlock();
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_loop_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("condition-checking loop")),
            "expected WAIT_NOT_GUARDED_BY_LOOP finding for Condition.await(), got {messages:?}"
        );
    }

    #[test]
    fn wait_not_guarded_by_loop_ignores_condition_await_nanos_inside_while() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;
import java.util.concurrent.locks.Condition;
import java.util.concurrent.locks.ReentrantLock;
public class ClassD {
    public void methodFour(boolean varOne) throws Exception {
        ReentrantLock varTwo = new ReentrantLock();
        Condition varThree = varTwo.newCondition();
        varTwo.lock();
        try {
            while (!varOne) {
                varThree.awaitNanos(10L);
                varOne = true;
            }
        } finally {
            varTwo.unlock();
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_loop_messages(&output);
        assert!(
            messages.is_empty(),
            "did not expect WAIT_NOT_GUARDED_BY_LOOP finding for while-guarded awaitNanos(): {messages:?}"
        );
    }

    #[test]
    fn wait_not_guarded_by_loop_reports_wait_with_timeout_under_if() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;
public class ClassE {
    public void methodFive(Object varOne, boolean varTwo) throws Exception {
        synchronized (varOne) {
            if (!varTwo) {
                varOne.wait(10L);
            }
        }
    }
}
"#
            .to_string(),
        }];

        let output = compile_and_analyze(&harness, &sources, &[]);
        let messages = wait_loop_messages(&output);
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("condition-checking loop")),
            "expected WAIT_NOT_GUARDED_BY_LOOP finding for wait(long), got {messages:?}"
        );
    }
}
