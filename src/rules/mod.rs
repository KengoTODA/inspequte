use anyhow::Result;
use serde_sarif::sarif::{
    ArtifactLocation, Location, LogicalLocation, Message, PhysicalLocation, Region,
    Result as SarifResult,
};

use crate::engine::AnalysisContext;

pub(crate) mod array_equals;
pub(crate) mod dead_code;
pub(crate) mod empty_catch;
pub(crate) mod ineffective_equals;
pub(crate) mod insecure_api;
pub(crate) mod nullness;
pub(crate) mod record_array_field;
pub(crate) mod slf4j_placeholder_mismatch;

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
