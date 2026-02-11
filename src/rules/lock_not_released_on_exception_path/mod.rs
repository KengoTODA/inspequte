use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::engine::AnalysisContext;
use crate::ir::{BasicBlock, Instruction, InstructionKind, Method};
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects lock acquisitions without guaranteed unlock on all reachable exits.
#[derive(Default)]
pub(crate) struct LockNotReleasedOnExceptionPathRule;

crate::register_rule!(LockNotReleasedOnExceptionPathRule);

/// Lock acquisition site metadata used for path exploration.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct LockSite {
    block_start: u32,
    instruction_index: usize,
    offset: u32,
}

/// Exploration state for CFG traversal after a lock acquisition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ExplorationState {
    block_start: u32,
    instruction_index: usize,
    unlock_seen: bool,
}

impl Rule for LockNotReleasedOnExceptionPathRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "LOCK_NOT_RELEASED_ON_EXCEPTION_PATH",
            name: "Lock acquired without guaranteed release",
            description: "Lock.lock() must be followed by unlock() on every reachable exit path",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }

            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }

            let class_results =
                context.with_span("rule.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }

                        let lock_sites = lock_sites(method);
                        if lock_sites.is_empty() {
                            continue;
                        }

                        let block_map = block_map(method);
                        let successor_map = successor_map(method);
                        let mut seen_offsets = BTreeSet::new();

                        for site in lock_sites {
                            if !seen_offsets.insert(site.offset) {
                                continue;
                            }
                            if has_exit_path_without_unlock(&block_map, &successor_map, site) {
                                let message = result_message(format!(
                                    "Lock acquired in {}.{}{} may exit without unlock(); release it in a finally block.",
                                    class.name, method.name, method.descriptor
                                ));
                                let line = method.line_for_offset(site.offset);
                                let artifact_uri = context.class_artifact_uri(class);
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
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }
        Ok(results)
    }
}

fn lock_sites(method: &Method) -> Vec<LockSite> {
    let mut sites = Vec::new();
    for block in &method.cfg.blocks {
        for (instruction_index, instruction) in block.instructions.iter().enumerate() {
            if is_lock_invocation(instruction) {
                sites.push(LockSite {
                    block_start: block.start_offset,
                    instruction_index,
                    offset: instruction.offset,
                });
            }
        }
    }
    sites.sort_by_key(|site| site.offset);
    sites
}

fn block_map(method: &Method) -> BTreeMap<u32, &BasicBlock> {
    let mut map = BTreeMap::new();
    for block in &method.cfg.blocks {
        map.insert(block.start_offset, block);
    }
    map
}

fn successor_map(method: &Method) -> BTreeMap<u32, Vec<u32>> {
    let mut map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for edge in &method.cfg.edges {
        map.entry(edge.from).or_default().push(edge.to);
    }
    for targets in map.values_mut() {
        targets.sort();
        targets.dedup();
    }
    map
}

fn has_exit_path_without_unlock(
    block_map: &BTreeMap<u32, &BasicBlock>,
    successor_map: &BTreeMap<u32, Vec<u32>>,
    site: LockSite,
) -> bool {
    let mut queue = VecDeque::new();
    let mut visited = BTreeSet::new();

    queue.push_back(ExplorationState {
        block_start: site.block_start,
        instruction_index: site.instruction_index + 1,
        unlock_seen: false,
    });

    while let Some(state) = queue.pop_front() {
        if !visited.insert(state) {
            continue;
        }

        let Some(block) = block_map.get(&state.block_start) else {
            continue;
        };

        let mut unlock_seen = state.unlock_seen;
        for instruction in block.instructions.iter().skip(state.instruction_index) {
            if is_unlock_invocation(instruction) {
                unlock_seen = true;
            }
        }

        let Some(successors) = successor_map.get(&state.block_start) else {
            if !unlock_seen {
                return true;
            }
            continue;
        };

        if successors.is_empty() {
            if !unlock_seen {
                return true;
            }
            continue;
        }

        for next in successors {
            queue.push_back(ExplorationState {
                block_start: *next,
                instruction_index: 0,
                unlock_seen,
            });
        }
    }

    false
}

fn is_lock_invocation(instruction: &Instruction) -> bool {
    let InstructionKind::Invoke(call) = &instruction.kind else {
        return false;
    };
    call.name == "lock"
        && call.descriptor == "()V"
        && matches!(
            call.owner.as_str(),
            "java/util/concurrent/locks/Lock" | "java/util/concurrent/locks/ReentrantLock"
        )
}

fn is_unlock_invocation(instruction: &Instruction) -> bool {
    let InstructionKind::Invoke(call) = &instruction.kind else {
        return false;
    };
    call.name == "unlock"
        && call.descriptor == "()V"
        && matches!(
            call.owner.as_str(),
            "java/util/concurrent/locks/Lock" | "java/util/concurrent/locks/ReentrantLock"
        )
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_sources_with_language(language: Language, sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(language, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| {
                result.rule_id.as_deref() == Some("LOCK_NOT_RELEASED_ON_EXCEPTION_PATH")
            })
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    fn analyze_sources(sources: Vec<SourceFile>) -> Vec<String> {
        analyze_sources_with_language(Language::Java, sources)
    }

    #[test]
    fn reports_lock_without_unlock_on_exception_path() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

public class ClassA {
    private final Lock varOne = new ReentrantLock();

    public void methodX(boolean varTwo) {
        varOne.lock();
        if (varTwo) {
            throw new IllegalStateException("tmpValue");
        }
        varOne.unlock();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("finally block"));
    }

    #[test]
    fn does_not_report_lock_released_in_finally() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

public class ClassB {
    private final Lock varOne = new ReentrantLock();

    public void methodY(boolean varTwo) {
        varOne.lock();
        try {
            if (varTwo) {
                System.out.println("tmpValue");
            }
        } finally {
            varOne.unlock();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn reports_only_unsafe_lock_site() {
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

import java.util.concurrent.locks.Lock;
import java.util.concurrent.locks.ReentrantLock;

public class ClassC {
    private final Lock varOne = new ReentrantLock();

    public void methodZ(boolean varTwo) {
        varOne.lock();
        try {
            if (varTwo) {
                System.out.println("tmpValue");
            }
        } finally {
            varOne.unlock();
        }

        varOne.lock();
        if (varTwo) {
            return;
        }
        varOne.unlock();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn does_not_report_when_no_lock_is_used() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

public class ClassD {
    public void methodW(boolean varOne) {
        if (varOne) {
            System.out.println("tmpValue");
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources(sources);
        assert!(messages.is_empty());
    }

    #[test]
    fn does_not_report_kotlin_with_lock_extension() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.kt".to_string(),
            contents: r#"
package com.example

import java.util.concurrent.locks.ReentrantLock
import kotlin.concurrent.withLock

class ClassE {
    private val varOne = ReentrantLock()

    fun methodQ(varTwo: Boolean): Int {
        return varOne.withLock {
            if (varTwo) {
                1
            } else {
                2
            }
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_sources_with_language(Language::Kotlin, sources);
        assert!(
            messages.is_empty(),
            "expected no LOCK_NOT_RELEASED_ON_EXCEPTION_PATH result, got: {messages:?}"
        );
    }
}
