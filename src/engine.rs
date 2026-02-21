use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use opentelemetry::Context as OtelContext;
use opentelemetry::KeyValue;
use rayon::prelude::*;
use serde_sarif::sarif::Artifact;
use serde_sarif::sarif::{MultiformatMessageString, ReportingDescriptor, Result as SarifResult};

use crate::ir::Class;
use crate::rules::{Rule, RuleMetadata};
use crate::telemetry::{Telemetry, with_span};

/// Inputs shared by analysis rules.
pub(crate) struct AnalysisContext {
    analysis_target_classes: Vec<Class>,
    dependency_classes: Vec<Class>,
    artifact_uris: BTreeMap<i64, String>,
    telemetry: Option<Arc<Telemetry>>,
    has_slf4j: bool,
    has_log4j2: bool,
}

/// Timing breakdown for context construction.
pub(crate) struct ContextTimings {
    pub(crate) call_graph_duration_ms: u128,
    pub(crate) artifact_duration_ms: u128,
    pub(crate) call_graph_hierarchy_duration_ms: u128,
    pub(crate) call_graph_index_duration_ms: u128,
    pub(crate) call_graph_edges_duration_ms: u128,
}

/// Analysis engine that executes configured rules.
pub(crate) struct Engine {
    rules: Vec<Box<dyn Rule + Sync>>,
}

impl Engine {
    pub(crate) fn new() -> Self {
        let mut rules = crate::rules::all_rules();
        rules.sort_by(|a, b| a.metadata().id.cmp(b.metadata().id));
        Self { rules }
    }

    pub(crate) fn analyze(&self, context: AnalysisContext) -> Result<EngineOutput> {
        let parent_context = OtelContext::current();
        let mut rule_outputs: Vec<RuleOutput> = self
            .rules
            .par_iter()
            .map(|rule| {
                let metadata = rule.metadata();
                let rule_span_attributes = [KeyValue::new("inspequte.rule_id", metadata.id)];
                let mut rule_results = match context.telemetry() {
                    Some(telemetry) => telemetry.in_span_with_parent(
                        &format!("rule:{}", metadata.id),
                        &rule_span_attributes,
                        &parent_context,
                        || rule.run(&context),
                    )?,
                    None => rule.run(&context)?,
                };
                for result in &mut rule_results {
                    if result.rule_id.is_none() {
                        result.rule_id = Some(metadata.id.to_string());
                    }
                }
                Ok(RuleOutput {
                    id: metadata.id.to_string(),
                    descriptor: rule_descriptor(&metadata),
                    results: rule_results,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        rule_outputs.sort_by(|left, right| left.id.cmp(&right.id));
        let mut rules = Vec::with_capacity(rule_outputs.len());
        let mut results = Vec::new();
        for output in rule_outputs {
            rules.push(output.descriptor);
            results.extend(output.results);
        }

        results.sort_by(|left, right| {
            let left_id = left.rule_id.as_deref().unwrap_or("");
            let right_id = right.rule_id.as_deref().unwrap_or("");
            let left_msg = left.message.text.as_deref().unwrap_or("").to_string();
            let right_msg = right.message.text.as_deref().unwrap_or("").to_string();
            left_id.cmp(right_id).then(left_msg.cmp(&right_msg))
        });

        Ok(EngineOutput { rules, results })
    }
}

struct RuleOutput {
    id: String,
    descriptor: ReportingDescriptor,
    results: Vec<SarifResult>,
}

/// Aggregated SARIF payload from rule execution.
pub(crate) struct EngineOutput {
    pub(crate) rules: Vec<ReportingDescriptor>,
    pub(crate) results: Vec<SarifResult>,
}

#[cfg(test)]
pub(crate) fn build_context(classes: Vec<Class>, artifacts: &[Artifact]) -> AnalysisContext {
    let (context, _) = build_context_with_timings(classes, artifacts, None);
    context
}

pub(crate) fn build_context_with_timings(
    classes: Vec<Class>,
    artifacts: &[Artifact],
    telemetry: Option<Arc<Telemetry>>,
) -> (AnalysisContext, ContextTimings) {
    let call_graph_duration_ms = 0;
    let artifact_started_at = Instant::now();
    let (analysis_target_artifacts, artifact_parents, artifact_uris) = with_span(
        telemetry.as_deref(),
        "artifact_analysis",
        &[KeyValue::new("inspequte.phase", "artifact_analysis")],
        || analyze_artifacts(artifacts),
    );
    let (has_slf4j, has_log4j2) = detect_logging_frameworks(&classes, telemetry.as_deref());
    let (analysis_target_classes, dependency_classes) =
        partition_classes(classes, &analysis_target_artifacts, &artifact_parents);
    let artifact_duration_ms = artifact_started_at.elapsed().as_millis();
    let timings = ContextTimings {
        call_graph_duration_ms,
        artifact_duration_ms,
        call_graph_hierarchy_duration_ms: 0,
        call_graph_index_duration_ms: 0,
        call_graph_edges_duration_ms: 0,
    };
    let context = AnalysisContext {
        analysis_target_classes,
        dependency_classes,
        artifact_uris,
        telemetry,
        has_slf4j,
        has_log4j2,
    };
    (context, timings)
}

fn rule_descriptor(metadata: &RuleMetadata) -> ReportingDescriptor {
    ReportingDescriptor::builder()
        .id(metadata.id)
        .name(metadata.name)
        .short_description(
            MultiformatMessageString::builder()
                .text(metadata.description)
                .build(),
        )
        .build()
}

impl AnalysisContext {
    pub(crate) fn analysis_target_classes(&self) -> &[Class] {
        &self.analysis_target_classes
    }

    #[allow(dead_code)]
    pub(crate) fn dependency_classes(&self) -> &[Class] {
        &self.dependency_classes
    }

    pub(crate) fn all_classes(&self) -> impl Iterator<Item = &Class> {
        self.analysis_target_classes
            .iter()
            .chain(self.dependency_classes.iter())
    }

    pub(crate) fn telemetry(&self) -> Option<&Telemetry> {
        self.telemetry.as_deref()
    }

    pub(crate) fn with_span<T, F>(&self, name: &str, attributes: &[KeyValue], f: F) -> T
    where
        F: FnOnce() -> T,
    {
        with_span(self.telemetry(), name, attributes, f)
    }

    pub(crate) fn artifact_uri(&self, index: i64) -> Option<&str> {
        self.artifact_uris.get(&index).map(|value| value.as_str())
    }

    pub(crate) fn class_artifact_uri(&self, class: &Class) -> Option<String> {
        let uri = self.artifact_uri(class.artifact_index)?;
        let class_uri = if uri.ends_with(".class") {
            uri.to_string()
        } else if uri.ends_with(".jar") {
            if uri.starts_with("jar:") {
                format!("{uri}!/{}.class", class.name)
            } else {
                format!("jar:{uri}!/{}.class", class.name)
            }
        } else {
            return None;
        };

        class_source_uri(&class_uri, class).or(Some(class_uri))
    }

    pub(crate) fn has_slf4j(&self) -> bool {
        self.has_slf4j
    }

    pub(crate) fn has_log4j2(&self) -> bool {
        self.has_log4j2
    }
}

fn class_source_uri(class_uri: &str, class: &Class) -> Option<String> {
    if !class_uri.ends_with(".class") {
        return None;
    }

    let source_name = class.source_file.as_deref()?;
    let prefix = class_uri.rsplit_once('/').map(|(prefix, _)| prefix)?;
    Some(format!("{prefix}/{source_name}"))
}

fn detect_logging_frameworks(classes: &[Class], telemetry: Option<&Telemetry>) -> (bool, bool) {
    let mut has_slf4j = false;
    let mut has_log4j2 = false;
    for class in classes {
        if has_slf4j && has_log4j2 {
            break;
        }
        if !has_slf4j || !has_log4j2 {
            for field in &class.fields {
                if !has_slf4j && contains_slf4j_type(&field.descriptor) {
                    has_slf4j = true;
                }
                if !has_log4j2 && contains_log4j2_type(&field.descriptor) {
                    has_log4j2 = true;
                }
            }
            for method in &class.methods {
                if !has_slf4j && contains_slf4j_type(&method.descriptor) {
                    has_slf4j = true;
                }
                if !has_log4j2 && contains_log4j2_type(&method.descriptor) {
                    has_log4j2 = true;
                }
            }
        }
        let mut references = class
            .referenced_classes
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        if let Some(super_name) = class.super_name.as_deref() {
            references.push(super_name);
        }
        for iface in &class.interfaces {
            references.push(iface);
        }
        for reference in references {
            if !has_slf4j
                && matches!(
                    reference,
                    "org/slf4j/Logger" | "org/slf4j/Marker" | "org/slf4j/LoggerFactory"
                )
            {
                has_slf4j = true;
            }
            if !has_log4j2
                && matches!(
                    reference,
                    "org/apache/logging/log4j/Logger"
                        | "org/apache/logging/log4j/LogManager"
                        | "org/apache/logging/log4j/Marker"
                        | "org/apache/logging/log4j/message/Message"
                )
            {
                has_log4j2 = true;
            }
        }
    }
    let attributes = [
        KeyValue::new("inspequte.slf4j.present", has_slf4j),
        KeyValue::new("inspequte.log4j2.present", has_log4j2),
    ];
    with_span(telemetry, "detect.logging_frameworks", &attributes, || {
        (has_slf4j, has_log4j2)
    })
}

fn contains_slf4j_type(descriptor: &str) -> bool {
    descriptor.contains("Lorg/slf4j/Logger;")
        || descriptor.contains("Lorg/slf4j/Marker;")
        || descriptor.contains("Lorg/slf4j/LoggerFactory;")
}

fn contains_log4j2_type(descriptor: &str) -> bool {
    descriptor.contains("Lorg/apache/logging/log4j/Logger;")
        || descriptor.contains("Lorg/apache/logging/log4j/Marker;")
        || descriptor.contains("Lorg/apache/logging/log4j/LogManager;")
        || descriptor.contains("Lorg/apache/logging/log4j/message/Message;")
}

fn analyze_artifacts(
    artifacts: &[Artifact],
) -> (BTreeSet<i64>, BTreeMap<i64, i64>, BTreeMap<i64, String>) {
    let mut analysis_targets = BTreeSet::new();
    let mut parents = BTreeMap::new();
    let mut uris = BTreeMap::new();
    for (index, artifact) in artifacts.iter().enumerate() {
        let index = index as i64;
        if let Some(location) = artifact.location.as_ref() {
            if let Some(uri) = location.uri.as_ref() {
                uris.insert(index, uri.clone());
            }
        }
        if let Some(parent) = artifact.parent_index {
            parents.insert(index, parent);
        }
        if let Some(roles) = &artifact.roles {
            if roles
                .iter()
                .any(|role| role.as_str() == Some("analysisTarget"))
            {
                analysis_targets.insert(index);
            }
        }
    }
    (analysis_targets, parents, uris)
}

fn partition_classes(
    classes: Vec<Class>,
    analysis_target_artifacts: &BTreeSet<i64>,
    artifact_parents: &BTreeMap<i64, i64>,
) -> (Vec<Class>, Vec<Class>) {
    if analysis_target_artifacts.is_empty() {
        return (classes, Vec::new());
    }

    let mut analysis_target_classes = Vec::new();
    let mut dependency_classes = Vec::new();
    for class in classes {
        if is_analysis_target_artifact(
            class.artifact_index,
            analysis_target_artifacts,
            artifact_parents,
        ) {
            analysis_target_classes.push(class);
        } else {
            dependency_classes.push(class);
        }
    }

    (analysis_target_classes, dependency_classes)
}

fn is_analysis_target_artifact(
    artifact_index: i64,
    analysis_target_artifacts: &BTreeSet<i64>,
    artifact_parents: &BTreeMap<i64, i64>,
) -> bool {
    let mut current = Some(artifact_index);
    while let Some(index) = current {
        if analysis_target_artifacts.contains(&index) {
            return true;
        }
        current = artifact_parents.get(&index).copied();
    }
    false
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serde_sarif::sarif::{ArtifactLocation, ArtifactRoles};

    use super::*;

    fn class_with_artifact(name: &str, artifact_index: i64) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            artifact_index,
            is_record: false,
        }
    }

    #[test]
    fn build_context_partitions_analysis_target_and_dependency_classes() {
        let classes = vec![
            class_with_artifact("com/example/ClassA", 0),
            class_with_artifact("com/example/ClassB", 1),
            class_with_artifact("com/example/ClassC", 2),
        ];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/app.jar".to_string())
                        .build(),
                )
                .roles(vec![json!(ArtifactRoles::AnalysisTarget)])
                .build(),
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/lib.jar".to_string())
                        .build(),
                )
                .build(),
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("jar:file:///tmp/app.jar!/lib/nested.jar".to_string())
                        .build(),
                )
                .parent_index(0)
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let analysis_target_names = context
            .analysis_target_classes()
            .iter()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>();
        let dependency_names = context
            .dependency_classes()
            .iter()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            analysis_target_names,
            vec!["com/example/ClassA", "com/example/ClassC"]
        );
        assert_eq!(dependency_names, vec!["com/example/ClassB"]);
    }

    #[test]
    fn build_context_treats_all_classes_as_targets_without_analysis_target_artifacts() {
        let classes = vec![
            class_with_artifact("com/example/ClassA", 0),
            class_with_artifact("com/example/ClassB", 1),
        ];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/app.jar".to_string())
                        .build(),
                )
                .build(),
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/lib.jar".to_string())
                        .build(),
                )
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let all_names = context
            .all_classes()
            .map(|class| class.name.as_str())
            .collect::<Vec<_>>();

        assert_eq!(context.analysis_target_classes().len(), 2);
        assert!(context.dependency_classes().is_empty());
        assert_eq!(all_names, vec!["com/example/ClassA", "com/example/ClassB"]);
    }

    #[test]
    fn class_artifact_uri_uses_source_file_name_for_class_artifact() {
        let classes = vec![Class {
            name: "com/example/ClassA".to_string(),
            source_file: Some("ClassA.java".to_string()),
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/build/classes/com/example/ClassA.class".to_string())
                        .build(),
                )
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let class = &context.analysis_target_classes()[0];
        assert_eq!(
            context.class_artifact_uri(class),
            Some("file:///tmp/build/classes/com/example/ClassA.java".to_string())
        );
    }

    #[test]
    fn class_artifact_uri_keeps_class_uri_when_source_file_is_missing() {
        let classes = vec![class_with_artifact("com/example/ClassA", 0)];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/build/classes/com/example/ClassA.class".to_string())
                        .build(),
                )
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let class = &context.analysis_target_classes()[0];
        assert_eq!(
            context.class_artifact_uri(class),
            Some("file:///tmp/build/classes/com/example/ClassA.class".to_string())
        );
    }

    #[test]
    fn class_artifact_uri_uses_source_file_attribute_name_when_available() {
        let classes = vec![Class {
            name: "com/example/FileAKt".to_string(),
            source_file: Some("file_a.kt".to_string()),
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/build/classes/com/example/FileAKt.class".to_string())
                        .build(),
                )
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let class = &context.analysis_target_classes()[0];
        assert_eq!(
            context.class_artifact_uri(class),
            Some("file:///tmp/build/classes/com/example/file_a.kt".to_string())
        );
    }

    #[test]
    fn class_artifact_uri_uses_outer_source_file_for_inner_class() {
        let classes = vec![Class {
            name: "com/example/ClassA$Inner".to_string(),
            source_file: Some("ClassA.java".to_string()),
            super_name: None,
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }];
        let artifacts = vec![
            Artifact::builder()
                .location(
                    ArtifactLocation::builder()
                        .uri("file:///tmp/build/classes/com/example/ClassA$Inner.class".to_string())
                        .build(),
                )
                .build(),
        ];

        let context = build_context(classes, &artifacts);
        let class = &context.analysis_target_classes()[0];
        assert_eq!(
            context.class_artifact_uri(class),
            Some("file:///tmp/build/classes/com/example/ClassA.java".to_string())
        );
    }
}
