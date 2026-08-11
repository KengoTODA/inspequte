use std::{env, fs};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::Value;

fn main() -> Result<()> {
    let mut arguments = env::args_os().skip(1);
    let schema_path = arguments.next().context("missing JSON Schema path")?;
    let instance_path = arguments.next().context("missing JSON instance path")?;
    if arguments.next().is_some() {
        bail!("expected exactly a schema path and an instance path");
    }

    let schema_text = fs::read_to_string(&schema_path)
        .with_context(|| format!("read JSON Schema {}", schema_path.to_string_lossy()))?;
    let schema: Value = serde_json::from_str(&schema_text)
        .with_context(|| format!("parse JSON Schema {}", schema_path.to_string_lossy()))?;
    let instance_text = fs::read_to_string(&instance_path)
        .with_context(|| format!("read JSON instance {}", instance_path.to_string_lossy()))?;
    let instance: Value = serde_json::from_str(&instance_text)
        .with_context(|| format!("parse JSON instance {}", instance_path.to_string_lossy()))?;

    let validator = jsonschema::validator_for(&schema).map_err(|error| {
        anyhow!(
            "compile JSON Schema {}: {error}",
            schema_path.to_string_lossy()
        )
    })?;
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|error| format!("{}: {error}", error.instance_path()))
        .collect();
    if !errors.is_empty() {
        bail!("JSON Schema validation failed:\n{}", errors.join("\n"));
    }
    Ok(())
}
