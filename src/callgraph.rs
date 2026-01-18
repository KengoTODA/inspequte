use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use crate::ir::{CallKind, Class};

/// Unique identifier for a method in the classpath.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct MethodId {
    pub(crate) class_name: String,
    pub(crate) name: String,
    pub(crate) descriptor: String,
}

/// Directed call edge between caller and callee.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct CallEdge {
    pub(crate) caller: Arc<MethodId>,
    pub(crate) callee: Arc<MethodId>,
    pub(crate) kind: CallKind,
    pub(crate) offset: u32,
}

/// Call graph built from CHA on the parsed classpath.
#[derive(Clone, Debug, Default)]
pub(crate) struct CallGraph {
    pub(crate) edges: Vec<CallEdge>,
}

/// Timing breakdown for call graph construction.
pub(crate) struct CallGraphTimings {
    pub(crate) hierarchy_duration_ms: u128,
    pub(crate) index_duration_ms: u128,
    pub(crate) edges_duration_ms: u128,
}

pub(crate) fn build_call_graph_with_timings(classes: &[Class]) -> (CallGraph, CallGraphTimings) {
    let hierarchy_started_at = Instant::now();
    let hierarchy = build_hierarchy(classes);
    let hierarchy_duration_ms = hierarchy_started_at.elapsed().as_millis();
    let index_started_at = Instant::now();
    let methods = index_methods(classes);
    let index_duration_ms = index_started_at.elapsed().as_millis();
    let edges_started_at = Instant::now();
    let edges = build_edges(classes, &hierarchy, &methods);
    let edges_duration_ms = edges_started_at.elapsed().as_millis();
    let timings = CallGraphTimings {
        hierarchy_duration_ms,
        index_duration_ms,
        edges_duration_ms,
    };
    (CallGraph { edges }, timings)
}

fn build_edges(
    classes: &[Class],
    hierarchy: &BTreeMap<String, Vec<String>>,
    methods: &MethodIndex,
) -> Vec<CallEdge> {
    let estimated_edges = classes
        .iter()
        .map(|class| {
            class
                .methods
                .iter()
                .map(|method| method.calls.len())
                .sum::<usize>()
        })
        .sum();
    let mut edges = Vec::with_capacity(estimated_edges);
    for class in classes {
        for method in &class.methods {
            let Some(caller) =
                lookup_method(methods, &class.name, &method.name, &method.descriptor)
            else {
                continue;
            };
            for call in &method.calls {
                match call.kind {
                    CallKind::Static | CallKind::Special => {
                        if let Some(callee) =
                            lookup_method(methods, &call.owner, &call.name, &call.descriptor)
                        {
                            edges.push(CallEdge {
                                caller: caller.clone(),
                                callee,
                                kind: call.kind,
                                offset: call.offset,
                            });
                        }
                    }
                    CallKind::Virtual | CallKind::Interface => {
                        if let Some(owner_candidate) =
                            lookup_method(methods, &call.owner, &call.name, &call.descriptor)
                        {
                            edges.push(CallEdge {
                                caller: caller.clone(),
                                callee: owner_candidate,
                                kind: call.kind,
                                offset: call.offset,
                            });
                        }
                        if let Some(descendants) = hierarchy.get(&call.owner) {
                            for class_name in descendants {
                                if let Some(candidate) =
                                    lookup_method(methods, class_name, &call.name, &call.descriptor)
                                {
                                    edges.push(CallEdge {
                                        caller: caller.clone(),
                                        callee: candidate,
                                        kind: call.kind,
                                        offset: call.offset,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

fn build_hierarchy(classes: &[Class]) -> BTreeMap<String, Vec<String>> {
    let mut hierarchy = BTreeMap::new();
    for class in classes {
        if let Some(super_name) = &class.super_name {
            hierarchy
                .entry(super_name.clone())
                .or_insert_with(Vec::new)
                .push(class.name.clone());
        }
    }
    for descendants in hierarchy.values_mut() {
        descendants.sort();
        descendants.dedup();
    }
    hierarchy
}

type MethodIndex = HashMap<String, HashMap<String, HashMap<String, Arc<MethodId>>>>;

fn index_methods(classes: &[Class]) -> MethodIndex {
    let mut map = HashMap::new();
    for class in classes {
        let class_entry = map.entry(class.name.clone()).or_insert_with(HashMap::new);
        for method in &class.methods {
            let name_entry = class_entry
                .entry(method.name.clone())
                .or_insert_with(HashMap::new);
            name_entry.insert(
                method.descriptor.clone(),
                Arc::new(MethodId {
                    class_name: class.name.clone(),
                    name: method.name.clone(),
                    descriptor: method.descriptor.clone(),
                }),
            );
        }
    }
    map
}

fn lookup_method(
    methods: &MethodIndex,
    class_name: &str,
    method_name: &str,
    descriptor: &str,
) -> Option<Arc<MethodId>> {
    methods
        .get(class_name)
        .and_then(|by_name| by_name.get(method_name))
        .and_then(|by_descriptor| by_descriptor.get(descriptor))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{CallSite, Method, MethodAccess, MethodNullness};

    fn class_with_method(name: &str, super_name: Option<&str>, method: &Method) -> Class {
        Class {
            name: name.to_string(),
            super_name: super_name.map(str::to_string),
            interfaces: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods: vec![method.clone()],
            artifact_index: 0,
            is_record: false,
        }
    }

    #[test]
    fn call_graph_includes_virtual_targets() {
        let caller = Method {
            name: "caller".to_string(),
            descriptor: "()V".to_string(),
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            nullness: MethodNullness::unknown(0),
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: crate::ir::ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: vec![CallSite {
                owner: "com/example/Base".to_string(),
                name: "target".to_string(),
                descriptor: "()V".to_string(),
                kind: CallKind::Virtual,
                offset: 0,
            }],
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
        };
        let base_method = Method {
            name: "target".to_string(),
            descriptor: "()V".to_string(),
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
            },
            nullness: MethodNullness::unknown(0),
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: crate::ir::ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
        };
        let subclass_method = base_method.clone();
        let classes = vec![
            class_with_method("com/example/Caller", None, &caller),
            class_with_method("com/example/Base", None, &base_method),
            class_with_method(
                "com/example/Sub",
                Some("com/example/Base"),
                &subclass_method,
            ),
        ];

        let (graph, _) = build_call_graph_with_timings(&classes);

        assert!(!graph.edges.is_empty());
    }
}
