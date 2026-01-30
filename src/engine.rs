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
use crate::rules::{
    Rule, RuleMetadata, array_equals::ArrayEqualsRule, empty_catch::EmptyCatchRule,
    ineffective_equals::IneffectiveEqualsRule, insecure_api::InsecureApiRule,
    interrupted_exception::InterruptedExceptionRule,
    log4j2_format_should_be_const::Log4j2FormatShouldBeConstRule, nullness::NullnessRule,
    prefer_enumset::PreferEnumSetRule, record_array_field::RecordArrayFieldRule,
    slf4j_format_should_be_const::Slf4jFormatShouldBeConstRule,
    slf4j_illegal_passed_class::Slf4jIllegalPassedClassRule,
    slf4j_logger_should_be_final::Slf4jLoggerShouldBeFinalRule,
    slf4j_logger_should_be_private::Slf4jLoggerShouldBePrivateRule,
    slf4j_manually_provided_message::Slf4jManuallyProvidedMessageRule,
    slf4j_placeholder_mismatch::Slf4jPlaceholderMismatchRule,
    slf4j_sign_only_format::Slf4jSignOnlyFormatRule, slf4j_unknown_array::Slf4jUnknownArrayRule,
};
use crate::telemetry::{Telemetry, with_span};

/// Inputs shared by analysis rules.
pub(crate) struct AnalysisContext {
    pub(crate) classes: Vec<Class>,
    artifact_uris: BTreeMap<i64, String>,
    analysis_target_artifacts: BTreeSet<i64>,
    artifact_parents: BTreeMap<i64, i64>,
    telemetry: Option<Arc<Telemetry>>,
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
        let mut rules: Vec<Box<dyn Rule + Sync>> = vec![
            Box::new(ArrayEqualsRule),
            Box::new(NullnessRule),
            Box::new(PreferEnumSetRule),
            Box::new(EmptyCatchRule),
            Box::new(InsecureApiRule),
            Box::new(IneffectiveEqualsRule),
            Box::new(RecordArrayFieldRule),
            Box::new(InterruptedExceptionRule),
            Box::new(Log4j2FormatShouldBeConstRule),
            Box::new(Slf4jFormatShouldBeConstRule),
            Box::new(Slf4jIllegalPassedClassRule),
            Box::new(Slf4jLoggerShouldBeFinalRule),
            Box::new(Slf4jLoggerShouldBePrivateRule),
            Box::new(Slf4jManuallyProvidedMessageRule),
            Box::new(Slf4jPlaceholderMismatchRule),
            Box::new(Slf4jSignOnlyFormatRule),
            Box::new(Slf4jUnknownArrayRule),
        ];
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
    let artifact_duration_ms = artifact_started_at.elapsed().as_millis();
    let timings = ContextTimings {
        call_graph_duration_ms,
        artifact_duration_ms,
        call_graph_hierarchy_duration_ms: 0,
        call_graph_index_duration_ms: 0,
        call_graph_edges_duration_ms: 0,
    };
    let context = AnalysisContext {
        classes,
        artifact_uris,
        analysis_target_artifacts,
        artifact_parents,
        telemetry,
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
    pub(crate) fn telemetry(&self) -> Option<&Telemetry> {
        self.telemetry.as_deref()
    }

    pub(crate) fn with_span<T, F>(&self, name: &str, attributes: &[KeyValue], f: F) -> T
    where
        F: FnOnce() -> T,
    {
        with_span(self.telemetry(), name, attributes, f)
    }

    pub(crate) fn is_analysis_target_class(&self, class: &Class) -> bool {
        if self.analysis_target_artifacts.is_empty() {
            return true;
        }
        let mut current = Some(class.artifact_index);
        while let Some(index) = current {
            if self.analysis_target_artifacts.contains(&index) {
                return true;
            }
            current = self.artifact_parents.get(&index).copied();
        }
        false
    }

    pub(crate) fn artifact_uri(&self, index: i64) -> Option<&str> {
        self.artifact_uris.get(&index).map(|value| value.as_str())
    }

    pub(crate) fn class_artifact_uri(&self, class: &Class) -> Option<String> {
        let uri = self.artifact_uri(class.artifact_index)?;
        if uri.ends_with(".class") {
            return Some(uri.to_string());
        }
        if uri.ends_with(".jar") {
            if uri.starts_with("jar:") {
                return Some(format!("{uri}!/{}.class", class.name));
            }
            return Some(format!("jar:{uri}!/{}.class", class.name));
        }
        None
    }
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
