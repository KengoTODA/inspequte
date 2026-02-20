use anyhow::Result;
use serde_sarif::sarif::{
    ArtifactLocation, Location, LogicalLocation, Message, PhysicalLocation, Region,
    Result as SarifResult,
};

use crate::engine::AnalysisContext;

pub(crate) mod array_equals;
pub(crate) mod bigdecimal_divide_without_rounding;
pub(crate) mod bigdecimal_equals_call;
pub(crate) mod bigdecimal_from_double;
pub(crate) mod bigdecimal_setscale_without_rounding;
pub(crate) mod boolean_getboolean_call;
pub(crate) mod compareto_overflow;
pub(crate) mod delete_on_exit_call;
pub(crate) mod deserialization_read_object_call;
pub(crate) mod empty_catch;
pub(crate) mod exception_cause_not_preserved;
pub(crate) mod explicit_gc_call;
pub(crate) mod future_get_without_timeout;
pub(crate) mod ineffective_equals;
pub(crate) mod insecure_api;
pub(crate) mod integer_getinteger_call;
pub(crate) mod interrupted_exception;
pub(crate) mod lock_not_released_on_exception_path;
pub(crate) mod log4j2_format_should_be_const;
pub(crate) mod log4j2_illegal_passed_class;
pub(crate) mod log4j2_logger_should_be_final;
pub(crate) mod log4j2_logger_should_be_private;
pub(crate) mod log4j2_manually_provided_message;
pub(crate) mod log4j2_sign_only_format;
pub(crate) mod log4j2_unknown_array;
pub(crate) mod long_getlong_call;
pub(crate) mod mutate_unmodifiable_collection;
pub(crate) mod nullness;
pub(crate) mod object_wait_without_timeout;
pub(crate) mod optional_get_call;
pub(crate) mod prefer_enumset;
pub(crate) mod print_stack_trace;
pub(crate) mod record_array_field;
pub(crate) mod return_in_finally;
pub(crate) mod run_finalization_call;
pub(crate) mod runtime_halt_call;
pub(crate) mod slf4j_format_should_be_const;
pub(crate) mod slf4j_illegal_passed_class;
pub(crate) mod slf4j_logger_should_be_final;
pub(crate) mod slf4j_logger_should_be_private;
pub(crate) mod slf4j_manually_provided_message;
pub(crate) mod slf4j_placeholder_mismatch;
pub(crate) mod slf4j_sign_only_format;
pub(crate) mod slf4j_unknown_array;
pub(crate) mod string_case_without_locale;
pub(crate) mod string_intern_call;
pub(crate) mod system_exit;
pub(crate) mod thread_run_direct_call;
pub(crate) mod url_equals_call;
pub(crate) mod url_hashcode_call;

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
    fn all_rules_registers_expected_rules() {
        let rules = all_rules();
        // Verify we have the expected number of rules
        assert_eq!(rules.len(), 50, "Expected 50 rules to be registered");

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
            "BIGDECIMAL_DIVIDE_WITHOUT_ROUNDING",
            "BIGDECIMAL_FROM_DOUBLE",
            "BIGDECIMAL_EQUALS_CALL",
            "BIGDECIMAL_SET_SCALE_WITHOUT_ROUNDING",
            "BOOLEAN_GETBOOLEAN_CALL",
            "COMPARETO_OVERFLOW",
            "DESERIALIZATION_READ_OBJECT_CALL",
            "DELETE_ON_EXIT_CALL",
            "EMPTY_CATCH",
            "EXPLICIT_GC_CALL",
            "EXCEPTION_CAUSE_NOT_PRESERVED",
            "INEFFECTIVE_EQUALS_HASHCODE",
            "INSECURE_API",
            "INTEGER_GETINTEGER_CALL",
            "INTERRUPTED_EXCEPTION_NOT_RESTORED",
            "FUTURE_GET_WITHOUT_TIMEOUT",
            "LOCK_NOT_RELEASED_ON_EXCEPTION_PATH",
            "LONG_GETLONG_CALL",
            "LOG4J2_FORMAT_SHOULD_BE_CONST",
            "LOG4J2_ILLEGAL_PASSED_CLASS",
            "LOG4J2_LOGGER_SHOULD_BE_FINAL",
            "LOG4J2_LOGGER_SHOULD_BE_PRIVATE",
            "LOG4J2_MANUALLY_PROVIDED_MESSAGE",
            "LOG4J2_SIGN_ONLY_FORMAT",
            "LOG4J2_UNKNOWN_ARRAY",
            "MUTATE_UNMODIFIABLE_COLLECTION",
            "NULLNESS",
            "OBJECT_WAIT_WITHOUT_TIMEOUT",
            "OPTIONAL_GET_CALL",
            "PREFER_ENUMSET",
            "PRINT_STACK_TRACE",
            "RECORD_ARRAY_FIELD",
            "RUN_FINALIZATION_CALL",
            "RETURN_IN_FINALLY",
            "RUNTIME_HALT_CALL",
            "SLF4J_FORMAT_SHOULD_BE_CONST",
            "SLF4J_ILLEGAL_PASSED_CLASS",
            "SLF4J_LOGGER_SHOULD_BE_FINAL",
            "SLF4J_LOGGER_SHOULD_BE_PRIVATE",
            "SLF4J_MANUALLY_PROVIDED_MESSAGE",
            "SLF4J_PLACEHOLDER_MISMATCH",
            "SLF4J_SIGN_ONLY_FORMAT",
            "SLF4J_UNKNOWN_ARRAY",
            "STRING_CASE_WITHOUT_LOCALE",
            "STRING_INTERN_CALL",
            "SYSTEM_EXIT",
            "THREAD_RUN_DIRECT_CALL",
            "URL_EQUALS_CALL",
            "URL_HASHCODE_CALL",
        ];

        for expected_id in &expected_ids {
            assert!(
                ids.contains(expected_id),
                "Expected rule ID {} not found",
                expected_id
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
