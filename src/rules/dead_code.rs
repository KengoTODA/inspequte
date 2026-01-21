use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::callgraph::MethodId;
use crate::engine::AnalysisContext;
use crate::ir::Method;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

/// Rule that detects unreachable methods.
pub(crate) struct DeadCodeRule;

impl Rule for DeadCodeRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "DEAD_CODE",
            name: "Dead code",
            description: "Unreachable methods detected by call graph",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut method_map = BTreeMap::new();
        let mut entry_points = Vec::new();

        for class in &context.classes {
            if !context.is_analysis_target_class(class) {
                continue;
            }
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            context.with_span("class", &attributes, || {
                for method in &class.methods {
                    let id = Arc::new(MethodId {
                        class_name: class.name.clone(),
                        name: method.name.clone(),
                        descriptor: method.descriptor.clone(),
                    });
                    let artifact_uri = context.class_artifact_uri(class);
                    method_map.insert(id.clone(), (class.name.clone(), method, artifact_uri));
                    if is_entry_method(method) {
                        entry_points.push(id);
                    }
                }
            });
        }

        if entry_points.is_empty() {
            return Ok(Vec::new());
        }

        let adjacency = build_adjacency(&context.call_graph.edges);
        let reachable = walk_graph(&entry_points, &adjacency);

        let mut results = Vec::new();
        for (id, (class_name, method, artifact_uri)) in method_map {
            if reachable.contains(&id) {
                continue;
            }
            if !method_has_body(method) {
                continue;
            }
            let message = result_message(format!(
                "Unreachable method: {}.{}{}",
                class_name, method.name, method.descriptor
            ));
            let line = method.line_for_offset(0);
            let location = method_location_with_line(
                &class_name,
                &method.name,
                &method.descriptor,
                artifact_uri.as_deref(),
                line,
            );
            results.push(
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location])
                    .build(),
            );
        }

        Ok(results)
    }
}

fn build_adjacency(
    edges: &[crate::callgraph::CallEdge],
) -> BTreeMap<Arc<MethodId>, Vec<Arc<MethodId>>> {
    let mut adjacency = BTreeMap::new();
    for edge in edges {
        adjacency
            .entry(edge.caller.clone())
            .or_insert_with(Vec::new)
            .push(edge.callee.clone());
    }
    adjacency
}

fn walk_graph(
    entries: &[Arc<MethodId>],
    adjacency: &BTreeMap<Arc<MethodId>, Vec<Arc<MethodId>>>,
) -> BTreeSet<Arc<MethodId>> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    for entry in entries {
        if visited.insert(entry.clone()) {
            queue.push_back(entry.clone());
        }
    }
    while let Some(node) = queue.pop_front() {
        if let Some(neighbors) = adjacency.get(&node) {
            for neighbor in neighbors {
                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }
    }
    visited
}

fn is_entry_method(method: &Method) -> bool {
    method.access.is_public
}

fn method_has_body(method: &Method) -> bool {
    !method.access.is_abstract && !method.bytecode.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classpath::resolve_classpath;
    use crate::descriptor::method_param_count;
    use crate::engine::build_context;
    use crate::ir::{CallKind, CallSite, Class, ControlFlowGraph, MethodAccess, MethodNullness};
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn empty_cfg() -> ControlFlowGraph {
        ControlFlowGraph {
            blocks: Vec::new(),
            edges: Vec::new(),
        }
    }

    fn method_with(
        name: &str,
        descriptor: &str,
        access: MethodAccess,
        bytecode: Vec<u8>,
        calls: Vec<CallSite>,
    ) -> Method {
        Method {
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            access,
            nullness: MethodNullness::unknown(method_param_count(descriptor).expect("param count")),
            bytecode,
            line_numbers: Vec::new(),
            cfg: empty_cfg(),
            calls,
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
        }
    }

    fn class_with_methods(name: &str, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            super_name: None,
            interfaces: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            artifact_index: 0,
            is_record: false,
        }
    }

    fn context_for(classes: Vec<Class>) -> crate::engine::AnalysisContext {
        let classpath = resolve_classpath(&classes).expect("classpath build");
        build_context(classes, classpath, &[])
    }

    #[test]
    fn dead_code_rule_reports_unreachable_method() {
        let main_method = method_with(
            "main",
            "([Ljava/lang/String;)V",
            MethodAccess {
                is_public: true,
                is_static: true,
                is_abstract: false,
            },
            vec![0],
            vec![CallSite {
                owner: "com/example/App".to_string(),
                name: "reachable".to_string(),
                descriptor: "()V".to_string(),
                kind: CallKind::Static,
                offset: 0,
            }],
        );
        let reachable = method_with(
            "reachable",
            "()V",
            MethodAccess {
                is_public: false,
                is_static: true,
                is_abstract: false,
            },
            vec![0],
            Vec::new(),
        );
        let unreachable = method_with(
            "unreachable",
            "()V",
            MethodAccess {
                is_public: false,
                is_static: false,
                is_abstract: false,
            },
            vec![0],
            Vec::new(),
        );
        let classes = vec![class_with_methods(
            "com/example/App",
            vec![main_method, reachable, unreachable],
        )];
        let context = context_for(classes);

        let results = DeadCodeRule.run(&context).expect("dead code rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("Unreachable method: com/example/App.unreachable()V"));
    }

    #[test]
    fn dead_code_rule_skips_when_no_entrypoints() {
        let helper = method_with(
            "helper",
            "()V",
            MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            vec![0],
            Vec::new(),
        );
        let classes = vec![class_with_methods("com/example/Util", vec![helper])];
        let context = context_for(classes);

        let results = DeadCodeRule.run(&context).expect("dead code rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn dead_code_rule_skips_methods_without_body() {
        let main_method = method_with(
            "main",
            "([Ljava/lang/String;)V",
            MethodAccess {
                is_public: true,
                is_static: true,
                is_abstract: false,
            },
            vec![0],
            Vec::new(),
        );
        let unreachable = method_with(
            "unreachable",
            "()V",
            MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            Vec::new(),
            Vec::new(),
        );
        let classes = vec![class_with_methods(
            "com/example/App",
            vec![main_method, unreachable],
        )];
        let context = context_for(classes);

        let results = DeadCodeRule.run(&context).expect("dead code rule run");

        assert!(results.is_empty());
    }

    #[test]
    fn dead_code_rule_reports_unreachable_method_from_harness() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/App.java".to_string(),
            contents: r#"
package com.example;
public class App {
    public void entry() {
        helper();
    }

    private void helper() {}

    private void unused() {}
}
"#
            .to_string(),
        }];

        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");

        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("DEAD_CODE"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(messages.iter().any(|msg| msg.contains("unused()V")));
    }
}
