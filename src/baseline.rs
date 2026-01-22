use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_sarif::sarif::{Location, Result as SarifResult};

/// Baseline data used to suppress known issues in subsequent scans.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Baseline {
    version: u32,
    findings: Vec<BaselineEntry>,
}

/// Canonicalized result entry stored in a baseline file.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
struct BaselineEntry {
    rule_id: String,
    message: String,
    locations: Vec<BaselineLocation>,
}

/// Minimal location snapshot for matching findings across runs.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
struct BaselineLocation {
    logical: Option<String>,
    uri: Option<String>,
    start_line: Option<i64>,
}

impl Baseline {
    pub(crate) fn capture(results: &[SarifResult]) -> Self {
        let mut findings = BTreeSet::new();
        for result in results {
            findings.insert(BaselineEntry::from(result));
        }
        Self {
            version: 1,
            findings: findings.into_iter().collect(),
        }
    }

    pub(crate) fn filter(&self, results: Vec<SarifResult>) -> Vec<SarifResult> {
        results
            .into_iter()
            .filter(|result| {
                let entry = BaselineEntry::from(result);
                self.findings.binary_search(&entry).is_err()
            })
            .collect()
    }
}

pub(crate) fn write_baseline(path: &Path, results: &[SarifResult]) -> Result<()> {
    let baseline = Baseline::capture(results);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create baseline directory {}", parent.display()))?;
    }
    let mut file = File::create(path)
        .with_context(|| format!("failed to create baseline file {}", path.display()))?;

    // Compact JSON with one finding per line for readable diffs.
    write!(file, "{{\"version\":{},\"findings\":[", baseline.version)
        .context("failed to write baseline header")?;
    for (index, finding) in baseline.findings.iter().enumerate() {
        file.write_all(b"\n")
            .context("failed to write baseline newline")?;
        serde_json::to_writer(&mut file, finding).context("failed to serialize baseline entry")?;
        if index + 1 < baseline.findings.len() {
            file.write_all(b",")
                .context("failed to write baseline separator")?;
        }
    }
    if !baseline.findings.is_empty() {
        file.write_all(b"\n")
            .context("failed to write baseline trailing newline")?;
    }
    file.write_all(b"]}\n")
        .context("failed to finalize baseline file")?;
    Ok(())
}

pub(crate) fn load_baseline(path: &Path) -> Result<Option<Baseline>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read baseline file {}", path.display()));
        }
    };
    let mut baseline: Baseline =
        serde_json::from_str(&content).context("failed to parse baseline file")?;
    baseline.findings.sort();
    baseline.findings.dedup();
    Ok(Some(baseline))
}

impl From<&SarifResult> for BaselineEntry {
    fn from(result: &SarifResult) -> Self {
        let rule_id = result.rule_id.as_deref().unwrap_or_default().to_string();
        let message = result
            .message
            .text
            .as_deref()
            .unwrap_or_default()
            .to_string();
        let mut locations = Vec::new();
        if let Some(result_locations) = result.locations.as_ref() {
            for location in result_locations {
                locations.push(BaselineLocation::from(location));
            }
        }
        locations.sort();
        Self {
            rule_id,
            message,
            locations,
        }
    }
}

impl From<&Location> for BaselineLocation {
    fn from(location: &Location) -> Self {
        let logical = location
            .logical_locations
            .as_ref()
            .and_then(|locs| locs.first())
            .and_then(|loc| loc.name.clone());
        let uri = location
            .physical_location
            .as_ref()
            .and_then(|physical| physical.artifact_location.as_ref())
            .and_then(|artifact| artifact.uri.clone());
        let start_line = location
            .physical_location
            .as_ref()
            .and_then(|physical| physical.region.as_ref())
            .and_then(|region| region.start_line);
        Self {
            logical,
            uri,
            start_line,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_sarif::sarif::{LogicalLocation, Message, Result as SarifResultBuilder};
    use tempfile::tempdir;

    fn sample_result(rule_id: &str, logical: &str, message: &str) -> SarifResult {
        SarifResultBuilder::builder()
            .rule_id(rule_id)
            .message(Message::builder().text(message.to_string()).build())
            .locations(vec![
                Location::builder()
                    .logical_locations(vec![
                        LogicalLocation::builder().name(logical.to_string()).build(),
                    ])
                    .build(),
            ])
            .build()
    }

    #[test]
    fn baseline_filters_matching_results() {
        let findings = vec![sample_result(
            "RULE_A",
            "com/example/App.run()V",
            "something",
        )];
        let baseline = Baseline::capture(&findings);

        let filtered = baseline.filter(findings);
        assert!(filtered.is_empty());
    }

    #[test]
    fn baseline_preserves_new_findings() {
        let existing = vec![sample_result(
            "RULE_A",
            "com/example/App.run()V",
            "something",
        )];
        let baseline = Baseline::capture(&existing);

        let new_findings = vec![sample_result(
            "RULE_A",
            "com/example/Other.run()V",
            "something",
        )];

        let filtered = baseline.filter(new_findings.clone());
        assert_eq!(new_findings, filtered);
    }

    #[test]
    fn baseline_round_trips_through_json() {
        let findings = vec![
            sample_result("RULE_A", "com/example/App.run()V", "one"),
            sample_result("RULE_B", "com/example/App.run()V", "two"),
        ];
        let baseline = Baseline::capture(&findings);

        let serialized = serde_json::to_string_pretty(&baseline).expect("serialize baseline");
        let parsed: Baseline = serde_json::from_str(&serialized).expect("parse baseline");

        let filtered = parsed.filter(findings);
        assert!(filtered.is_empty());
    }

    #[test]
    fn baseline_write_and_load_round_trip() {
        let findings = vec![
            sample_result("RULE_A", "com/example/App.run()V", "one"),
            sample_result("RULE_B", "com/example/App.run()V", "two"),
        ];
        let dir = tempdir().expect("baseline temp dir");
        let path = dir.path().join("baseline.json");

        write_baseline(&path, &findings).expect("write baseline");
        let loaded = load_baseline(&path).expect("load baseline");

        let baseline = loaded.expect("baseline present");
        let filtered = baseline.filter(findings);
        assert!(filtered.is_empty());
    }

    #[test]
    fn baseline_load_missing_file_returns_none() {
        let dir = tempdir().expect("baseline temp dir");
        let path = dir.path().join("missing.json");

        let loaded = load_baseline(&path).expect("load baseline");

        assert!(loaded.is_none());
    }
}
