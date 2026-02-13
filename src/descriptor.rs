use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};

/// Parsed summary of a JVM method descriptor.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct MethodDescriptorSummary {
    pub(crate) param_count: usize,
    pub(crate) return_kind: ReturnKind,
}

/// Parse a JVM method descriptor once and return its key summary fields.
pub(crate) fn method_descriptor_summary(descriptor: &str) -> Result<MethodDescriptorSummary> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse method descriptor")?;
    let return_kind = match descriptor.return_type() {
        TypeDescriptor::Void => ReturnKind::Void,
        TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _) => ReturnKind::Reference,
        _ => ReturnKind::Primitive,
    };
    Ok(MethodDescriptorSummary {
        param_count: descriptor.parameter_types().len(),
        return_kind,
    })
}

/// Count parameters in a JVM method descriptor.
pub(crate) fn method_param_count(descriptor: &str) -> Result<usize> {
    Ok(method_descriptor_summary(descriptor)?.param_count)
}

/// Return kind of a JVM method descriptor.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum ReturnKind {
    Void,
    Primitive,
    Reference,
}

/// Determine the return kind from a JVM method descriptor.
pub(crate) fn method_return_kind(descriptor: &str) -> Result<ReturnKind> {
    Ok(method_descriptor_summary(descriptor)?.return_kind)
}
