use anyhow::Result;
use serde_sarif::sarif::{
    ArtifactLocation, Location, LogicalLocation, Message, PhysicalLocation, Region,
    Result as SarifResult,
};

use crate::engine::AnalysisContext;

pub(crate) mod array_equals;
pub(crate) mod empty_catch;
pub(crate) mod ineffective_equals;
pub(crate) mod insecure_api;
pub(crate) mod interrupted_exception;
pub(crate) mod log4j2_format_should_be_const;
pub(crate) mod nullness;
pub(crate) mod prefer_enumset;
pub(crate) mod record_array_field;
pub(crate) mod slf4j_format_should_be_const;
pub(crate) mod slf4j_illegal_passed_class;
pub(crate) mod slf4j_logger_should_be_final;
pub(crate) mod slf4j_logger_should_be_private;
pub(crate) mod slf4j_manually_provided_message;
pub(crate) mod slf4j_placeholder_mismatch;
pub(crate) mod slf4j_sign_only_format;
pub(crate) mod slf4j_unknown_array;

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
    if let (Some(uri), Some(line)) = (artifact_uri, line) {
        if uri.ends_with(".class") {
            let region = Region::builder().start_line(line as i64).build();
            let artifact_location = ArtifactLocation::builder().uri(uri.to_string()).build();
            let physical = PhysicalLocation::builder()
                .artifact_location(artifact_location)
                .region(region)
                .build();
            return Location::builder()
                .logical_locations(vec![logical])
                .physical_location(physical)
                .build();
        }
    }
    Location::builder().logical_locations(vec![logical]).build()
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

pub(crate) fn class_location(class_name: &str) -> Location {
    let logical = LogicalLocation::builder()
        .name(class_name)
        .kind("type")
        .build();
    Location::builder().logical_locations(vec![logical]).build()
}

pub(crate) fn result_message(text: impl Into<String>) -> Message {
    Message::builder().text(text.into()).build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_rules_registers_expected_rules() {
        let rules = all_rules();
        // Verify we have the expected number of rules
        assert_eq!(rules.len(), 17, "Expected 17 rules to be registered");

        // Verify all rule IDs are unique
        let mut ids: Vec<_> = rules.iter().map(|r| r.metadata().id).collect();
        ids.sort();
        let unique_count = ids.len();
        ids.dedup();
        assert_eq!(
            ids.len(),
            unique_count,
            "Rule IDs should be unique, found duplicates"
        );

        // Verify expected rule IDs are present
        let expected_ids = [
            "ARRAY_EQUALS",
            "EMPTY_CATCH",
            "INEFFECTIVE_EQUALS_HASHCODE",
            "INSECURE_API",
            "INTERRUPTED_EXCEPTION_NOT_RESTORED",
            "LOG4J2_FORMAT_SHOULD_BE_CONST",
            "NULLNESS",
            "PREFER_ENUMSET",
            "RECORD_ARRAY_FIELD",
            "SLF4J_FORMAT_SHOULD_BE_CONST",
            "SLF4J_ILLEGAL_PASSED_CLASS",
            "SLF4J_LOGGER_SHOULD_BE_FINAL",
            "SLF4J_LOGGER_SHOULD_BE_PRIVATE",
            "SLF4J_MANUALLY_PROVIDED_MESSAGE",
            "SLF4J_PLACEHOLDER_MISMATCH",
            "SLF4J_SIGN_ONLY_FORMAT",
            "SLF4J_UNKNOWN_ARRAY",
        ];

        for expected_id in &expected_ids {
            assert!(
                ids.contains(expected_id),
                "Expected rule ID {} not found",
                expected_id
            );
        }
    }
}
