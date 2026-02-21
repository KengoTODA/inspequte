use anyhow::Result;
use serde_sarif::sarif::{
    ArtifactLocation, Location, LogicalLocation, Message, PhysicalLocation, Region,
    Result as SarifResult,
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
}
