use anyhow::Result;
use serde_sarif::sarif::{
    ArtifactLocation, CodeFlow, Location, LogicalLocation, Message, PhysicalLocation, Region,
    Result as SarifResult, ThreadFlow, ThreadFlowLocation,
};

use crate::engine::AnalysisContext;

// Rule modules are auto-discovered by build.rs — do not edit manually.
include!(concat!(env!("OUT_DIR"), "/rule_modules.rs"));

/// Metadata describing an analysis rule.
#[derive(Clone, Debug)]
pub(crate) struct RuleMetadata {
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
}

/// Rule interface for analysis execution.
pub(crate) trait Rule {
    fn metadata(&self) -> RuleMetadata;
    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>>;
}

/// Output-size limits applied while converting rule evidence to SARIF.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EvidenceLimits {
    max_related_locations: usize,
    max_thread_flow_locations: usize,
}

impl EvidenceLimits {
    /// Creates evidence limits for related locations and ordered flow steps.
    pub(crate) fn new(max_related_locations: usize, max_thread_flow_locations: usize) -> Self {
        Self {
            max_related_locations,
            max_thread_flow_locations,
        }
    }
}

impl Default for EvidenceLimits {
    fn default() -> Self {
        Self::new(8, 12)
    }
}

/// A named program point used as related or ordered witness evidence.
#[derive(Clone, Debug)]
pub(crate) struct EvidenceStep {
    location: Location,
    message: String,
    kinds: Vec<String>,
    nesting_level: Option<i64>,
}

impl EvidenceStep {
    /// Creates an evidence step with optional SARIF flow metadata.
    pub(crate) fn new(
        location: Location,
        message: impl Into<String>,
        kinds: impl IntoIterator<Item = impl Into<String>>,
        nesting_level: Option<i64>,
    ) -> Self {
        Self {
            location,
            message: message.into(),
            kinds: kinds.into_iter().map(Into::into).collect(),
            nesting_level,
        }
    }
}

/// Rule-owned evidence before it is bounded and converted to SARIF objects.
#[derive(Clone, Debug)]
pub(crate) struct ResultEvidence {
    description: String,
    thread_id: String,
    related_locations: Vec<EvidenceStep>,
    witness: Vec<EvidenceStep>,
}

impl ResultEvidence {
    /// Creates one deterministic evidence path and its named related locations.
    pub(crate) fn new(
        description: impl Into<String>,
        thread_id: impl Into<String>,
        related_locations: Vec<EvidenceStep>,
        witness: Vec<EvidenceStep>,
    ) -> Self {
        Self {
            description: description.into(),
            thread_id: thread_id.into(),
            related_locations,
            witness,
        }
    }

    /// Applies output limits and converts the evidence to SARIF 2.1.0 objects.
    pub(crate) fn to_sarif(&self, limits: EvidenceLimits) -> SarifEvidence {
        let related_locations = self
            .related_locations
            .iter()
            .take(limits.max_related_locations)
            .enumerate()
            .map(|(id, step)| {
                let mut location = step.location.clone();
                location.id = Some(id as i64);
                location.message = Some(result_message(step.message.clone()));
                location
            })
            .collect();

        let (witness, omitted) = bounded_witness(&self.witness, limits.max_thread_flow_locations);
        let code_flows = if witness.is_empty() {
            Vec::new()
        } else {
            let locations: Vec<ThreadFlowLocation> = witness
                .into_iter()
                .enumerate()
                .map(|(execution_order, step)| {
                    let mut location = step.location.clone();
                    location.id = None;
                    location.message = Some(result_message(step.message.clone()));
                    let mut flow_location = ThreadFlowLocation::builder()
                        .execution_order(execution_order as i64)
                        .kinds(step.kinds.clone())
                        .location(location)
                        .build();
                    flow_location.nesting_level = step.nesting_level;
                    flow_location
                })
                .collect();
            let thread_flow = ThreadFlow::builder()
                .id(self.thread_id.clone())
                .locations(locations)
                .build();
            let description = if omitted == 0 {
                self.description.clone()
            } else {
                format!(
                    "{} {omitted} intermediate evidence step(s) omitted by the evidence limit.",
                    self.description
                )
            };
            vec![
                CodeFlow::builder()
                    .message(result_message(description))
                    .thread_flows(vec![thread_flow])
                    .build(),
            ]
        };

        SarifEvidence {
            related_locations,
            code_flows,
        }
    }
}

/// SARIF evidence fields ready to attach to a result.
#[derive(Clone, Debug)]
pub(crate) struct SarifEvidence {
    pub(crate) related_locations: Vec<Location>,
    pub(crate) code_flows: Vec<CodeFlow>,
}

fn bounded_witness(steps: &[EvidenceStep], limit: usize) -> (Vec<&EvidenceStep>, usize) {
    if steps.len() <= limit {
        return (steps.iter().collect(), 0);
    }
    if limit == 0 {
        return (Vec::new(), steps.len());
    }
    if limit == 1 {
        return (
            vec![steps.last().expect("non-empty witness")],
            steps.len() - 1,
        );
    }

    let head = limit.div_ceil(2);
    let tail = limit - head;
    let mut bounded = Vec::with_capacity(limit);
    bounded.extend(steps[..head].iter());
    bounded.extend(steps[steps.len() - tail..].iter());
    (bounded, steps.len() - limit)
}

/// Wrapper struct for rule factory functions to enable inventory collection.
pub(crate) struct RuleFactory(pub fn() -> Box<dyn Rule + Sync>);

inventory::collect!(RuleFactory);

/// Macro to register a rule implementation.
///
/// Usage: `register_rule!(RuleName);`
/// This macro creates a factory function and registers it with inventory.
#[macro_export]
macro_rules! register_rule {
    ($rule_type:ty) => {
        inventory::submit! {
            $crate::rules::RuleFactory(|| Box::new(<$rule_type>::default()))
        }
    };
}

/// Returns all registered rules as boxed trait objects.
pub(crate) fn all_rules() -> Vec<Box<dyn Rule + Sync>> {
    inventory::iter::<RuleFactory>
        .into_iter()
        .map(|factory| (factory.0)())
        .collect()
}

pub(crate) fn method_location_with_line(
    class_name: &str,
    method_name: &str,
    descriptor: &str,
    artifact_uri: Option<&str>,
    line: Option<u32>,
) -> Location {
    let logical = method_logical_location(class_name, method_name, descriptor);
    if let Some(uri) = artifact_uri {
        if uri.ends_with(".class") {
            let container_uri = jar_container_uri(uri);
            let artifact_uri = container_uri.as_deref().unwrap_or(uri);
            let artifact_location = ArtifactLocation::builder()
                .uri(artifact_uri.to_string())
                .build();
            let physical = if container_uri.is_none() {
                if let Some(line) = line {
                    let region = Region::builder().start_line(line as i64).build();
                    PhysicalLocation::builder()
                        .artifact_location(artifact_location)
                        .region(region)
                        .build()
                } else {
                    PhysicalLocation::builder()
                        .artifact_location(artifact_location)
                        .build()
                }
            } else {
                PhysicalLocation::builder()
                    .artifact_location(artifact_location)
                    .build()
            };
            return Location::builder()
                .logical_locations(vec![logical])
                .physical_location(physical)
                .build();
        }
        let artifact_location = ArtifactLocation::builder().uri(uri.to_string()).build();
        let physical = if let Some(line) = line {
            let region = Region::builder().start_line(line as i64).build();
            PhysicalLocation::builder()
                .artifact_location(artifact_location)
                .region(region)
                .build()
        } else {
            PhysicalLocation::builder()
                .artifact_location(artifact_location)
                .build()
        };
        return Location::builder()
            .logical_locations(vec![logical])
            .physical_location(physical)
            .build();
    }
    Location::builder().logical_locations(vec![logical]).build()
}

fn jar_container_uri(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("jar:")?;
    let container = rest.split("!/").next()?;
    Some(container.to_string())
}

pub(crate) fn method_logical_location(
    class_name: &str,
    method_name: &str,
    descriptor: &str,
) -> LogicalLocation {
    LogicalLocation::builder()
        .name(format!("{class_name}.{method_name}{descriptor}"))
        .kind("function")
        .build()
}

pub(crate) fn class_location(class_name: &str, artifact_uri: Option<&str>) -> Location {
    let logical = LogicalLocation::builder()
        .name(class_name)
        .kind("type")
        .build();
    if let Some(uri) = artifact_uri {
        if uri.ends_with(".class") {
            let container_uri = jar_container_uri(uri);
            let artifact_uri = container_uri.as_deref().unwrap_or(uri);
            let artifact_location = ArtifactLocation::builder()
                .uri(artifact_uri.to_string())
                .build();
            let physical = PhysicalLocation::builder()
                .artifact_location(artifact_location)
                .build();
            return Location::builder()
                .logical_locations(vec![logical])
                .physical_location(physical)
                .build();
        }
        let artifact_location = ArtifactLocation::builder().uri(uri.to_string()).build();
        let physical = PhysicalLocation::builder()
            .artifact_location(artifact_location)
            .build();
        return Location::builder()
            .logical_locations(vec![logical])
            .physical_location(physical)
            .build();
    }
    Location::builder().logical_locations(vec![logical]).build()
}

pub(crate) fn result_message(text: impl Into<String>) -> Message {
    Message::builder().text(text.into()).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rules_have_unique_ids() {
        let rules = all_rules();
        assert!(!rules.is_empty(), "At least one rule must be registered");

        let mut ids: Vec<_> = rules.iter().map(|r| r.metadata().id).collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total, "Rule IDs must be unique");
    }

    #[test]
    fn all_rules_have_non_empty_metadata() {
        for rule in all_rules() {
            let meta = rule.metadata();
            assert!(!meta.id.is_empty(), "Rule ID must not be empty");
            assert!(!meta.name.is_empty(), "Rule name must not be empty");
            assert!(
                !meta.description.is_empty(),
                "Rule description must not be empty"
            );
        }
    }

    #[test]
    fn jar_container_uri_extracts_container() {
        let uri = "jar:file:///tmp/app.jar!/com/example/ClassA.class";
        assert_eq!(
            jar_container_uri(uri),
            Some("file:///tmp/app.jar".to_string())
        );
    }

    fn evidence_step(name: &str) -> EvidenceStep {
        EvidenceStep::new(
            method_location_with_line("ClassA", name, "()V", None, None),
            format!("Visit {name}."),
            ["call", "function"],
            None,
        )
    }

    #[test]
    fn evidence_limits_preserve_witness_endpoints_deterministically() {
        let steps = ["methodA", "methodB", "methodC", "methodD", "methodE"]
            .map(evidence_step)
            .to_vec();
        let evidence = ResultEvidence::new("Witness path.", "thread-a", steps.clone(), steps)
            .to_sarif(EvidenceLimits::new(2, 3));

        assert_eq!(evidence.related_locations.len(), 2);
        assert_eq!(evidence.related_locations[0].id, Some(0));
        assert_eq!(evidence.related_locations[1].id, Some(1));

        let code_flow = &evidence.code_flows[0];
        assert_eq!(
            code_flow
                .message
                .as_ref()
                .and_then(|message| message.text.as_deref()),
            Some("Witness path. 2 intermediate evidence step(s) omitted by the evidence limit.")
        );
        let locations = &code_flow.thread_flows[0].locations;
        assert_eq!(locations.len(), 3);
        let names: Vec<_> = locations
            .iter()
            .filter_map(|step| step.location.as_ref())
            .filter_map(|location| location.logical_locations.as_ref())
            .filter_map(|locations| locations.first())
            .filter_map(|location| location.name.as_deref())
            .collect();
        assert_eq!(
            names,
            [
                "ClassA.methodA()V",
                "ClassA.methodB()V",
                "ClassA.methodE()V"
            ]
        );
        assert_eq!(
            locations
                .iter()
                .map(|location| location.execution_order)
                .collect::<Vec<_>>(),
            [Some(0), Some(1), Some(2)]
        );
    }
}
