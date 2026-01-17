use std::str::FromStr;

use anyhow::{Context, Result};
use jdescriptor::{MethodDescriptor, TypeDescriptor};

/// Count parameters in a JVM method descriptor.
pub(crate) fn method_param_count(descriptor: &str) -> Result<usize> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse method descriptor")?;
    Ok(descriptor.parameter_types().len())
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
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse method descriptor")?;
    let kind = match descriptor.return_type() {
        TypeDescriptor::Void => ReturnKind::Void,
        TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _) => ReturnKind::Reference,
        _ => ReturnKind::Primitive,
    };
    Ok(kind)
}
