mod baseline;
mod cfg;
mod classpath;
mod dataflow;
mod descriptor;
mod engine;
mod ir;
mod opcodes;
mod rules;
mod scan;
mod telemetry;
#[cfg(test)]
mod test_harness;

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use opentelemetry::KeyValue;
use serde_json::json;
use serde_sarif::sarif::Result as SarifResult;
use serde_sarif::sarif::{
    Artifact, Invocation, PropertyBag, ReportingDescriptor, Run, RunAutomationDetails, SCHEMA_URL,
    Sarif, Tool, ToolComponent,
};
use tracing::error;

use crate::baseline::{load_baseline, write_baseline};
use crate::classpath::resolve_classpath;
use crate::engine::{Engine, build_context_with_timings};
use crate::scan::scan_inputs;
use crate::telemetry::{Telemetry, current_trace_id, init_logging, with_span};

const DEFAULT_BASELINE_PATH: &str = ".inspequte/baseline.json";

/// CLI arguments for inspequte execution.
#[derive(Parser, Debug)]
#[command(
    name = "inspequte",
    about = "Fast, deterministic SARIF output for JVM class files and JAR files analysis.",
    version,
    subcommand_negates_reqs = true
)]
struct Cli {
    #[command(flatten)]
    scan: ScanArgs,
    #[command(subcommand)]
    command: Option<Command>,
}

/// Options for running a scan.
#[derive(Args, Debug, Clone)]
struct ScanArgs {
    #[command(flatten)]
    input: InputArgs,
    #[arg(long, value_name = "PATH")]
    output: Option<PathBuf>,
    #[arg(
        long,
        value_name = "ID",
        help = "Optional SARIF run.automationDetails.id (GitHub code scanning category)."
    )]
    automation_details_id: Option<String>,
    #[arg(
        long,
        value_name = "URL",
        help = "OTLP HTTP collector URL (recommended: http://localhost:4318/)."
    )]
    otel: Option<String>,
    #[arg(
        long,
        value_name = "RULE_ID[,RULE_ID...]|@PATH",
        action = clap::ArgAction::Append,
        help = "Rule IDs to run. Accepts comma-separated IDs and @file references (one rule ID per line). Repeatable."
    )]
    rules: Vec<String>,
    #[arg(long, value_name = "PATH", default_value = DEFAULT_BASELINE_PATH)]
    baseline: PathBuf,
    #[arg(
        long,
        help = "Warn instead of failing when the same class name appears in multiple inputs. The class from the lexicographically first artifact path is used."
    )]
    allow_duplicate_classes: bool,
}

/// Input configuration shared by all commands.
#[derive(Args, Debug, Clone)]
struct InputArgs {
    #[arg(
        long,
        value_name = "PATH",
        required = true,
        num_args = 1..,
        help = "Input class/JAR/directory paths. Use @file to read paths (one per line)."
    )]
    input: Vec<String>,
    #[arg(
        long,
        value_name = "PATH",
        num_args = 1..,
        help = "Classpath entries. Use @file to read paths (one per line)."
    )]
    classpath: Vec<String>,
}

/// Expanded input configuration after resolving @file references.
#[derive(Debug, Clone)]
struct ExpandedInputArgs {
    input: Vec<PathBuf>,
    classpath: Vec<PathBuf>,
}

/// Subcommands supported by the CLI.
#[derive(Subcommand, Debug)]
enum Command {
    /// Create a baseline file containing all current findings.
    Baseline(BaselineArgs),
}

/// Arguments for creating a baseline file.
#[derive(Args, Debug, Clone)]
struct BaselineArgs {
    #[command(flatten)]
    input: InputArgs,
    #[arg(long, value_name = "PATH", default_value = DEFAULT_BASELINE_PATH)]
    output: PathBuf,
    #[arg(
        long,
        value_name = "URL",
        help = "OTLP HTTP collector URL (recommended: http://localhost:4318/)."
    )]
    otel: Option<String>,
    #[arg(
        long,
        help = "Warn instead of failing when the same class name appears in multiple inputs. The class from the lexicographically first artifact path is used."
    )]
    allow_duplicate_classes: bool,
}

fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err:?}");
            std::process::ExitCode::from(1)
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Some(Command::Baseline(args)) => run_baseline(args),
        None => run_scan(cli.scan),
    }
}

fn run_scan(args: ScanArgs) -> Result<()> {
    let expanded = expand_input_args(&args.input)?;
    let selected_rule_ids = expand_rule_args(&args.rules)?;
    let root_span_name = build_root_span_name(&expanded.input);
    let root_span_attributes = build_root_span_attributes("scan", &expanded.input);

    let telemetry = match &args.otel {
        Some(url) => Some(Arc::new(Telemetry::new(url.clone())?)),
        None => None,
    };
    init_logging();
    let result = with_span(
        telemetry.as_deref(),
        &root_span_name,
        &root_span_attributes,
        || {
            if let Some(trace_id) = current_trace_id() {
                eprintln!("trace-id={trace_id}");
            }
            let mut analysis = analyze(
                &expanded.input,
                &expanded.classpath,
                selected_rule_ids.as_ref(),
                telemetry.clone(),
                args.allow_duplicate_classes,
            )?;
            let analysis_ref = &mut analysis;
            let baseline_result = with_span(
                telemetry.as_deref(),
                "baseline",
                &[KeyValue::new("inspequte.phase", "baseline")],
                || -> Result<()> {
                    if let Some(baseline) = load_baseline(&args.baseline)? {
                        let filtered = baseline.filter(std::mem::take(&mut analysis_ref.results));
                        analysis_ref.results = filtered;
                    }
                    Ok(())
                },
            );
            baseline_result?;
            with_span(
                telemetry.as_deref(),
                "sarif",
                &[KeyValue::new("inspequte.phase", "sarif")],
                || -> Result<()> {
                    let invocation = build_invocation(&analysis.invocation_stats);
                    let sarif = build_sarif(
                        telemetry.as_deref(),
                        analysis.artifacts,
                        invocation,
                        analysis.rules,
                        analysis.results,
                        args.automation_details_id.clone(),
                    );
                    if should_validate_sarif() {
                        validate_sarif(&sarif)?;
                    }
                    let write_result = with_span(
                        telemetry.as_deref(),
                        "sarif.write",
                        &[KeyValue::new("inspequte.phase", "write")],
                        || -> Result<()> {
                            let mut writer = output_writer(args.output.as_deref())?;
                            serde_json::to_writer(&mut writer, &sarif)
                                .context("failed to serialize SARIF output")?;
                            writer
                                .write_all(b"\n")
                                .context("failed to write SARIF output")?;
                            Ok(())
                        },
                    );
                    write_result?;
                    Ok(())
                },
            )?;

            Ok(())
        },
    );

    if let Some(telemetry) = telemetry {
        if let Err(err) = telemetry.shutdown() {
            error!("telemetry shutdown failed: {err}");
        }
    }

    result
}

fn run_baseline(args: BaselineArgs) -> Result<()> {
    let expanded = expand_input_args(&args.input)?;
    let root_span_name = build_root_span_name(&expanded.input);
    let root_span_attributes = build_root_span_attributes("baseline", &expanded.input);
    let telemetry = match &args.otel {
        Some(url) => Some(Arc::new(Telemetry::new(url.clone())?)),
        None => None,
    };
    init_logging();
    let result = with_span(
        telemetry.as_deref(),
        &root_span_name,
        &root_span_attributes,
        || -> Result<()> {
            if let Some(trace_id) = current_trace_id() {
                eprintln!("trace-id={trace_id}");
            }
            let analysis = analyze(
                &expanded.input,
                &expanded.classpath,
                None,
                telemetry.clone(),
                args.allow_duplicate_classes,
            )?;
            write_baseline(&args.output, &analysis.results)?;
            Ok(())
        },
    );
    if let Some(telemetry) = telemetry {
        if let Err(err) = telemetry.shutdown() {
            error!("telemetry shutdown failed: {err}");
        }
    }
    result
}

fn expand_input_args(args: &InputArgs) -> Result<ExpandedInputArgs> {
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let input =
        expand_path_args(&args.input, &base_dir).context("failed to expand --input arguments")?;
    let input = filter_missing_paths("input", input)?;
    if input.is_empty() {
        anyhow::bail!("no input paths provided");
    }
    let classpath = expand_path_args(&args.classpath, &base_dir)
        .context("failed to expand --classpath arguments")?;
    let classpath = filter_missing_paths("classpath entry", classpath)?;
    Ok(ExpandedInputArgs { input, classpath })
}

fn expand_path_args(args: &[String], base_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut expanded = Vec::new();
    let mut stack = Vec::new();
    for arg in args {
        expanded.extend(expand_path_arg(arg, base_dir, &mut stack)?);
    }
    Ok(expanded)
}

fn expand_path_arg(arg: &str, base_dir: &Path, stack: &mut Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let Some(path_str) = arg.strip_prefix('@') else {
        return Ok(vec![PathBuf::from(arg)]);
    };
    if path_str.is_empty() {
        anyhow::bail!("empty @file reference");
    }
    let file_path = PathBuf::from(path_str);
    let resolved = if file_path.is_absolute() {
        file_path
    } else {
        base_dir.join(file_path)
    };
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", resolved.display()))?;
    if stack.contains(&canonical) {
        anyhow::bail!("circular @file reference: {}", canonical.display());
    }
    let content = fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read {}", canonical.display()))?;
    stack.push(canonical.clone());
    let file_dir = canonical.parent().unwrap_or_else(|| Path::new(""));
    let mut paths = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            paths.extend(expand_path_arg(line, file_dir, stack)?);
            continue;
        }
        let entry = PathBuf::from(line);
        let resolved_entry = if entry.is_absolute() {
            entry
        } else {
            file_dir.join(entry)
        };
        paths.push(resolved_entry);
    }
    stack.pop();
    Ok(paths)
}

fn filter_missing_paths(label: &str, paths: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut filtered = Vec::new();
    for path in paths {
        if path.exists() {
            filtered.push(path);
            continue;
        }
        if path.extension().is_none() {
            continue;
        }
        anyhow::bail!("{label} not found: {}", path.display());
    }
    Ok(filtered)
}

/// Aggregated analysis output before SARIF serialization.
struct AnalysisOutput {
    artifacts: Vec<Artifact>,
    invocation_stats: InvocationStats,
    rules: Vec<ReportingDescriptor>,
    results: Vec<SarifResult>,
}

fn analyze(
    input: &[PathBuf],
    classpath: &[PathBuf],
    selected_rule_ids: Option<&BTreeSet<String>>,
    telemetry: Option<Arc<Telemetry>>,
    allow_duplicate_classes: bool,
) -> Result<AnalysisOutput> {
    let scan_started_at = Instant::now();
    let scan = with_span(
        telemetry.as_deref(),
        "scan",
        &[KeyValue::new("inspequte.phase", "scan")],
        || scan_inputs(input, classpath, telemetry.as_deref()),
    )?;
    let scan_duration_ms = scan_started_at.elapsed().as_millis();
    let artifact_count = scan.artifacts.len();
    let classpath_started_at = Instant::now();
    let classpath_index = with_span(
        telemetry.as_deref(),
        "classpath",
        &[KeyValue::new("inspequte.phase", "classpath")],
        || resolve_classpath(&scan.classes, &scan.artifacts, allow_duplicate_classes),
    )?;
    let classpath_duration_ms = classpath_started_at.elapsed().as_millis();
    let classpath_class_count = classpath_index.classes.len();
    let artifacts = scan.artifacts;
    let classes = scan.classes;
    let (context, context_timings) =
        build_context_with_timings(classes, &artifacts, telemetry.clone());
    let analysis_rules_started_at = Instant::now();
    let engine = Engine::new_with_allowed_rule_ids(selected_rule_ids)?;
    let analysis = with_span(
        telemetry.as_deref(),
        "analysis_rules",
        &[KeyValue::new("inspequte.phase", "analysis_rules")],
        || engine.analyze(context),
    )?;
    let analysis_rules_duration_ms = analysis_rules_started_at.elapsed().as_millis();
    let invocation_stats = InvocationStats {
        scan_duration_ms,
        classpath_duration_ms,
        analysis_call_graph_duration_ms: context_timings.call_graph_duration_ms,
        analysis_artifact_duration_ms: context_timings.artifact_duration_ms,
        analysis_call_graph_hierarchy_duration_ms: context_timings.call_graph_hierarchy_duration_ms,
        analysis_call_graph_index_duration_ms: context_timings.call_graph_index_duration_ms,
        analysis_call_graph_edges_duration_ms: context_timings.call_graph_edges_duration_ms,
        analysis_rules_duration_ms,
        class_count: scan.class_count,
        artifact_count,
        classpath_class_count,
    };

    Ok(AnalysisOutput {
        artifacts,
        invocation_stats,
        rules: analysis.rules,
        results: analysis.results,
    })
}

fn expand_rule_args(args: &[String]) -> Result<Option<BTreeSet<String>>> {
    if args.is_empty() {
        return Ok(None);
    }
    let mut rules = BTreeSet::new();
    let mut stack = Vec::new();
    let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for arg in args {
        collect_rules_from_cli_arg(arg, &base_dir, &mut stack, &mut rules)?;
    }
    if rules.is_empty() {
        anyhow::bail!("--rules was provided but no rule IDs were found");
    }
    Ok(Some(rules))
}

fn collect_rules_from_cli_arg(
    arg: &str,
    base_dir: &Path,
    stack: &mut Vec<PathBuf>,
    rules: &mut BTreeSet<String>,
) -> Result<()> {
    for token in arg.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        if let Some(path_str) = token.strip_prefix('@') {
            collect_rules_from_file(path_str, base_dir, stack, rules)?;
            continue;
        }
        rules.insert(token.to_string());
    }
    Ok(())
}

fn collect_rules_from_file(
    path_str: &str,
    base_dir: &Path,
    stack: &mut Vec<PathBuf>,
    rules: &mut BTreeSet<String>,
) -> Result<()> {
    if path_str.is_empty() {
        anyhow::bail!("empty @file reference in --rules");
    }

    let file_path = PathBuf::from(path_str);
    let resolved = if file_path.is_absolute() {
        file_path
    } else {
        base_dir.join(file_path)
    };
    let canonical = resolved
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", resolved.display()))?;
    if stack.contains(&canonical) {
        anyhow::bail!(
            "circular @file reference in --rules: {}",
            canonical.display()
        );
    }
    let content = fs::read_to_string(&canonical)
        .with_context(|| format!("failed to read {}", canonical.display()))?;
    stack.push(canonical.clone());
    let file_dir = canonical.parent().unwrap_or_else(|| Path::new(""));
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(nested_path) = line.strip_prefix('@') {
            collect_rules_from_file(nested_path, file_dir, stack, rules)?;
            continue;
        }
        if line.contains(',') {
            anyhow::bail!(
                "invalid --rules file entry '{}' in {}: use one rule ID per line",
                line,
                canonical.display()
            );
        }
        rules.insert(line.to_string());
    }
    stack.pop();
    Ok(())
}

fn output_writer(output: Option<&Path>) -> Result<Box<dyn Write>> {
    match output {
        Some(path) if path == Path::new("-") => Ok(Box::new(io::stdout())),
        Some(path) => {
            Ok(Box::new(File::create(path).with_context(|| {
                format!("failed to open {}", path.display())
            })?))
        }
        None => Ok(Box::new(io::stdout())),
    }
}

fn build_root_span_attributes(command: &str, inputs: &[PathBuf]) -> Vec<KeyValue> {
    let target_count = inputs.len();
    let target_kind = classify_target_kind(inputs);
    let primary_name = primary_target_name(inputs);
    let primary_hash = primary_target_hash(inputs);
    let target_label = target_label(&primary_name, &primary_hash, target_count);

    vec![
        KeyValue::new("inspequte.command", command.to_string()),
        KeyValue::new("inspequte.target.kind", target_kind.to_string()),
        KeyValue::new("inspequte.target.count", target_count as i64),
        KeyValue::new("inspequte.target.primary_name", primary_name),
        KeyValue::new("inspequte.target.primary_hash", primary_hash),
        KeyValue::new("inspequte.target.label", target_label),
    ]
}

fn build_root_span_name(inputs: &[PathBuf]) -> String {
    let primary_name = primary_target_name(inputs);
    if primary_name.is_empty() {
        return "execution".to_string();
    }
    let target_count = inputs.len();
    if target_count <= 1 {
        return format!("execution {primary_name}");
    }
    format!("execution {primary_name}(+{})", target_count - 1)
}

fn classify_target_kind(inputs: &[PathBuf]) -> &'static str {
    let mut kinds = BTreeSet::new();
    for path in inputs {
        let kind = if path.is_dir() {
            "dir"
        } else {
            match path.extension().and_then(|ext| ext.to_str()) {
                Some(ext) if ext.eq_ignore_ascii_case("jar") => "jar",
                Some(ext) if ext.eq_ignore_ascii_case("class") => "class",
                _ => "mixed",
            }
        };
        kinds.insert(kind);
    }

    if kinds.len() != 1 {
        return "mixed";
    }
    kinds.into_iter().next().unwrap_or("mixed")
}

fn primary_target_name(inputs: &[PathBuf]) -> String {
    inputs.first().map_or_else(String::new, |path| {
        path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    })
}

fn primary_target_hash(inputs: &[PathBuf]) -> String {
    inputs
        .first()
        .map(|path| canonical_path_hash_short(path.as_path()))
        .unwrap_or_else(String::new)
}

fn target_label(primary_name: &str, primary_hash: &str, target_count: usize) -> String {
    if target_count <= 1 {
        return format!("{primary_name}#{primary_hash}");
    }
    format!("{primary_name}#{primary_hash}(+{})", target_count - 1)
}

fn canonical_path_hash_short(path: &Path) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let hash_input = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let mut hash = FNV_OFFSET_BASIS;
    for byte in hash_input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    let full = format!("{hash:016x}");
    full[..8].to_string()
}

/// Metadata captured for SARIF invocation properties.
struct InvocationStats {
    scan_duration_ms: u128,
    classpath_duration_ms: u128,
    analysis_call_graph_duration_ms: u128,
    analysis_artifact_duration_ms: u128,
    analysis_call_graph_hierarchy_duration_ms: u128,
    analysis_call_graph_index_duration_ms: u128,
    analysis_call_graph_edges_duration_ms: u128,
    analysis_rules_duration_ms: u128,
    class_count: usize,
    artifact_count: usize,
    classpath_class_count: usize,
}

fn build_invocation(stats: &InvocationStats) -> Invocation {
    let arguments: Vec<String> = std::env::args().collect();
    let command_line = arguments.join(" ");
    let mut properties = BTreeMap::new();
    properties.insert(
        "inspequte.scan_ms".to_string(),
        json!(stats.scan_duration_ms),
    );
    properties.insert(
        "inspequte.classpath_ms".to_string(),
        json!(stats.classpath_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_callgraph_ms".to_string(),
        json!(stats.analysis_call_graph_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_callgraph_hierarchy_ms".to_string(),
        json!(stats.analysis_call_graph_hierarchy_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_callgraph_index_ms".to_string(),
        json!(stats.analysis_call_graph_index_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_callgraph_edges_ms".to_string(),
        json!(stats.analysis_call_graph_edges_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_artifact_ms".to_string(),
        json!(stats.analysis_artifact_duration_ms),
    );
    properties.insert(
        "inspequte.analysis_rules_ms".to_string(),
        json!(stats.analysis_rules_duration_ms),
    );
    properties.insert(
        "inspequte.class_count".to_string(),
        json!(stats.class_count),
    );
    properties.insert(
        "inspequte.artifact_count".to_string(),
        json!(stats.artifact_count),
    );
    properties.insert(
        "inspequte.classpath_class_count".to_string(),
        json!(stats.classpath_class_count),
    );

    Invocation::builder()
        .execution_successful(true)
        .arguments(arguments)
        .command_line(command_line)
        .properties(
            PropertyBag::builder()
                .additional_properties(properties)
                .build(),
        )
        .build()
}

fn should_validate_sarif() -> bool {
    std::env::var("INSPEQUTE_VALIDATE_SARIF")
        .ok()
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn validate_sarif(sarif: &Sarif) -> Result<()> {
    let schema = serde_json::from_str(include_str!("assets/sarif-2.1.0.json"))
        .context("load SARIF schema")?;
    let compiled = jsonschema::validator_for(&schema)
        .map_err(|err| anyhow::anyhow!("compile SARIF schema: {err}"))?;
    let value = serde_json::to_value(sarif).context("serialize SARIF")?;
    let errors: Vec<String> = compiled
        .iter_errors(&value)
        .map(|error| error.to_string())
        .collect();
    if !errors.is_empty() {
        let message = errors.join("\n");
        anyhow::bail!("SARIF schema validation failed:\n{message}");
    }
    Ok(())
}

fn build_sarif(
    telemetry: Option<&Telemetry>,
    artifacts: Vec<Artifact>,
    invocation: Invocation,
    rules: Vec<ReportingDescriptor>,
    results: Vec<SarifResult>,
    automation_details_id: Option<String>,
) -> Sarif {
    with_span(telemetry, "sarif.build", &[], || {
        let semantic_version = env!("CARGO_PKG_VERSION").to_string();
        let driver = if rules.is_empty() {
            ToolComponent::builder()
                .name("inspequte")
                .information_uri("https://github.com/KengoTODA/inspequte")
                .semantic_version(semantic_version.clone())
                .build()
        } else {
            ToolComponent::builder()
                .name("inspequte")
                .information_uri("https://github.com/KengoTODA/inspequte")
                .rules(rules)
                .semantic_version(semantic_version)
                .build()
        };
        let tool = Tool {
            driver,
            extensions: None,
            properties: None,
        };
        let automation_details = automation_details_id.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(
                    RunAutomationDetails::builder()
                        .id(trimmed.to_string())
                        .build(),
                )
            }
        });
        let run = match (artifacts, automation_details) {
            (artifacts, Some(automation_details)) if artifacts.is_empty() => Run::builder()
                .tool(tool)
                .invocations(vec![invocation])
                .results(results)
                .automation_details(automation_details)
                .build(),
            (artifacts, None) if artifacts.is_empty() => Run::builder()
                .tool(tool)
                .invocations(vec![invocation])
                .results(results)
                .build(),
            (artifacts, Some(automation_details)) => Run::builder()
                .tool(tool)
                .invocations(vec![invocation])
                .results(results)
                .artifacts(artifacts)
                .automation_details(automation_details)
                .build(),
            (artifacts, None) => Run::builder()
                .tool(tool)
                .invocations(vec![invocation])
                .results(results)
                .artifacts(artifacts)
                .build(),
        };

        Sarif::builder()
            .schema(SCHEMA_URL)
            .runs(vec![run])
            .version(json!("2.1.0"))
            .build()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::engine::{Engine, build_context};
    use crate::scan::scan_inputs;

    fn attr_value<'a>(attributes: &'a [KeyValue], key: &str) -> &'a Value {
        attributes
            .iter()
            .find(|attribute| attribute.key.as_str() == key)
            .map(|attribute| &attribute.value)
            .unwrap_or_else(|| panic!("missing key: {key}"))
    }

    fn make_temp_test_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn root_span_attributes_for_single_jar_input() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let jar_path = temp_dir.join("ClassA.jar");
        fs::write(&jar_path, b"test").expect("write jar");

        let attributes = build_root_span_attributes("scan", std::slice::from_ref(&jar_path));

        assert_eq!(
            attr_value(&attributes, "inspequte.command"),
            &Value::from("scan")
        );
        assert_eq!(
            attr_value(&attributes, "inspequte.target.kind"),
            &Value::from("jar")
        );
        assert_eq!(
            attr_value(&attributes, "inspequte.target.count"),
            &Value::from(1_i64)
        );
        assert_eq!(
            attr_value(&attributes, "inspequte.target.primary_name"),
            &Value::from("ClassA.jar")
        );

        let primary_hash = attr_value(&attributes, "inspequte.target.primary_hash")
            .as_str()
            .into_owned();
        assert_eq!(primary_hash.len(), 8);
        assert!(primary_hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            attr_value(&attributes, "inspequte.target.label"),
            &Value::from(format!("ClassA.jar#{primary_hash}"))
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_attributes_for_single_class_input() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let class_path = temp_dir.join("ClassA.class");
        fs::write(&class_path, b"test").expect("write class");

        let attributes = build_root_span_attributes("scan", std::slice::from_ref(&class_path));

        assert_eq!(
            attr_value(&attributes, "inspequte.target.kind"),
            &Value::from("class")
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_attributes_for_multiple_inputs_use_mixed_kind_and_count_suffix() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let jar_path = temp_dir.join("ClassA.jar");
        fs::write(&jar_path, b"test").expect("write jar");
        let class_dir = temp_dir.join("classes");
        fs::create_dir_all(&class_dir).expect("create class dir");
        let class_path = temp_dir.join("ClassB.class");
        fs::write(&class_path, b"test").expect("write class");

        let attributes = build_root_span_attributes(
            "scan",
            &[jar_path.clone(), class_dir.clone(), class_path.clone()],
        );
        let primary_hash = attr_value(&attributes, "inspequte.target.primary_hash")
            .as_str()
            .into_owned();

        assert_eq!(
            attr_value(&attributes, "inspequte.target.kind"),
            &Value::from("mixed")
        );
        assert_eq!(
            attr_value(&attributes, "inspequte.target.count"),
            &Value::from(3_i64)
        );
        assert_eq!(
            attr_value(&attributes, "inspequte.target.label"),
            &Value::from(format!("ClassA.jar#{primary_hash}(+2)"))
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_attributes_include_baseline_command() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let jar_path = temp_dir.join("ClassA.jar");
        fs::write(&jar_path, b"test").expect("write jar");

        let attributes = build_root_span_attributes("baseline", std::slice::from_ref(&jar_path));

        assert_eq!(
            attr_value(&attributes, "inspequte.command"),
            &Value::from("baseline")
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_name_includes_primary_target_name() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let jar_path = temp_dir.join("ClassA.jar");
        fs::write(&jar_path, b"test").expect("write jar");

        let span_name = build_root_span_name(std::slice::from_ref(&jar_path));

        assert_eq!(span_name, "execution ClassA.jar");

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_name_includes_primary_target_name_and_count_suffix() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let first_path = temp_dir.join("ClassA.jar");
        fs::write(&first_path, b"test").expect("write first");
        let second_path = temp_dir.join("ClassB.jar");
        fs::write(&second_path, b"test").expect("write second");
        let third_path = temp_dir.join("ClassC.jar");
        fs::write(&third_path, b"test").expect("write third");

        let span_name =
            build_root_span_name(&[first_path.clone(), second_path.clone(), third_path.clone()]);
        let reordered = build_root_span_name(&[second_path, first_path, third_path]);

        assert_eq!(span_name, "execution ClassA.jar(+2)");
        assert_eq!(span_name, "execution ClassA.jar(+2)");
        assert_ne!(span_name, reordered);

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn root_span_attributes_are_deterministic_and_primary_target_comes_from_first_input() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let first_path = temp_dir.join("ClassA.jar");
        fs::write(&first_path, b"test").expect("write first");
        let second_path = temp_dir.join("ClassB.jar");
        fs::write(&second_path, b"test").expect("write second");

        let attributes_one =
            build_root_span_attributes("scan", &[first_path.clone(), second_path.clone()]);
        let attributes_two =
            build_root_span_attributes("scan", &[first_path.clone(), second_path.clone()]);
        let reordered = build_root_span_attributes("scan", &[second_path, first_path]);

        assert_eq!(
            attr_value(&attributes_one, "inspequte.target.primary_name"),
            &Value::from("ClassA.jar")
        );
        assert_eq!(
            attr_value(&attributes_one, "inspequte.target.label"),
            attr_value(&attributes_two, "inspequte.target.label")
        );
        assert_ne!(
            attr_value(&attributes_one, "inspequte.target.label"),
            attr_value(&reordered, "inspequte.target.label")
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn expand_path_args_reads_files_and_resolves_relative_entries() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let canonical_temp_dir = temp_dir.canonicalize().expect("canonicalize temp dir");

        let nested_path = temp_dir.join("nested.txt");
        fs::write(&nested_path, "lib/dependency.jar\n").expect("write nested");

        let inputs_path = temp_dir.join("inputs.txt");
        let mut inputs_file = fs::File::create(&inputs_path).expect("create inputs");
        writeln!(inputs_file, "# input classes").expect("write comment");
        writeln!(inputs_file, "classes").expect("write classes");
        writeln!(inputs_file, "@nested.txt").expect("write nested ref");
        writeln!(inputs_file, "").expect("write blank line");

        let args = vec![format!("@{}", inputs_path.display())];
        let expanded = expand_path_args(&args, Path::new(".")).expect("expand inputs");

        assert_eq!(
            expanded,
            vec![
                canonical_temp_dir.join("classes"),
                canonical_temp_dir.join("lib").join("dependency.jar")
            ]
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn expand_path_args_errors_on_missing_file() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let args = vec![format!("@{}", temp_dir.join("missing.txt").display())];
        let result = expand_path_args(&args, Path::new("."));

        assert!(result.is_err());

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn filter_missing_paths_ignores_missing_directory() {
        let temp_dir = make_temp_test_dir();
        let existing = temp_dir.join("classes");
        fs::create_dir_all(&existing).expect("create classes dir");
        let missing = temp_dir.join("missing-dir");

        let filtered =
            filter_missing_paths("input", vec![existing.clone(), missing]).expect("filter paths");

        assert_eq!(filtered, vec![existing]);
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn filter_missing_paths_rejects_missing_file() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let missing = temp_dir.join("missing.jar");

        let result = filter_missing_paths("classpath entry", vec![missing]);

        assert!(result.is_err());
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn cli_accepts_automation_details_id_option() {
        let cli = Cli::try_parse_from([
            "inspequte",
            "--input",
            "target/classes",
            "--automation-details-id",
            "inspequte/./main",
        ])
        .expect("parse CLI");

        assert_eq!(
            cli.scan.automation_details_id.as_deref(),
            Some("inspequte/./main")
        );
    }

    #[test]
    fn cli_accepts_repeatable_rules_option() {
        let cli = Cli::try_parse_from([
            "inspequte",
            "--input",
            "target/classes",
            "--rules",
            "SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL",
            "--rules",
            "RETURN_IN_FINALLY",
        ])
        .expect("parse CLI");

        assert_eq!(
            cli.scan.rules,
            vec![
                "SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL".to_string(),
                "RETURN_IN_FINALLY".to_string()
            ]
        );
    }

    #[test]
    fn expand_rule_args_supports_comma_separated_and_repeatable_values() {
        let args = vec![
            "SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL".to_string(),
            "RETURN_IN_FINALLY".to_string(),
        ];

        let expanded = expand_rule_args(&args).expect("expand rule args");

        assert_eq!(
            expanded,
            Some(BTreeSet::from([
                "RETURN_IN_FINALLY".to_string(),
                "SYSTEM_EXIT".to_string(),
                "THREAD_RUN_DIRECT_CALL".to_string(),
            ]))
        );
    }

    #[test]
    fn expand_rule_args_supports_at_file() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let rules_file = temp_dir.join("rules.txt");
        fs::write(
            &rules_file,
            "# selected rules\nSYSTEM_EXIT\nTHREAD_RUN_DIRECT_CALL\nRETURN_IN_FINALLY\n",
        )
        .expect("write rules file");

        let args = vec![format!("@{}", rules_file.display())];
        let expanded = expand_rule_args(&args).expect("expand rule args");

        assert_eq!(
            expanded,
            Some(BTreeSet::from([
                "RETURN_IN_FINALLY".to_string(),
                "SYSTEM_EXIT".to_string(),
                "THREAD_RUN_DIRECT_CALL".to_string(),
            ]))
        );

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn expand_rule_args_rejects_comma_separated_line_in_rules_file() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let rules_file = temp_dir.join("rules.txt");
        fs::write(&rules_file, "SYSTEM_EXIT,THREAD_RUN_DIRECT_CALL\n").expect("write rules");

        let args = vec![format!("@{}", rules_file.display())];
        let result = expand_rule_args(&args);

        assert!(result.is_err());
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn sarif_is_minimal_and_valid_shape() {
        let invocation = build_invocation(&InvocationStats {
            scan_duration_ms: 0,
            classpath_duration_ms: 0,
            analysis_call_graph_duration_ms: 0,
            analysis_artifact_duration_ms: 0,
            analysis_call_graph_hierarchy_duration_ms: 0,
            analysis_call_graph_index_duration_ms: 0,
            analysis_call_graph_edges_duration_ms: 0,
            analysis_rules_duration_ms: 0,
            class_count: 0,
            artifact_count: 0,
            classpath_class_count: 0,
        });
        let sarif = build_sarif(None, Vec::new(), invocation, Vec::new(), Vec::new(), None);
        let value = serde_json::to_value(&sarif).expect("serialize SARIF");

        assert_eq!(value["version"], "2.1.0");
        assert_eq!(value["$schema"], SCHEMA_URL);
        assert_eq!(value["runs"][0]["tool"]["driver"]["name"], "inspequte");
        assert_eq!(
            value["runs"][0]["tool"]["driver"]["semanticVersion"],
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            value["runs"][0]["tool"]["driver"]["informationUri"],
            "https://github.com/KengoTODA/inspequte"
        );
        assert!(
            value["runs"][0]["results"]
                .as_array()
                .expect("results array")
                .is_empty()
        );
        assert_eq!(
            value["runs"][0]["invocations"][0]["executionSuccessful"],
            true
        );
        assert!(value["runs"][0]["automationDetails"].is_null());
    }

    #[test]
    fn sarif_includes_automation_details_id_when_requested() {
        let invocation = build_invocation(&InvocationStats {
            scan_duration_ms: 0,
            classpath_duration_ms: 0,
            analysis_call_graph_duration_ms: 0,
            analysis_artifact_duration_ms: 0,
            analysis_call_graph_hierarchy_duration_ms: 0,
            analysis_call_graph_index_duration_ms: 0,
            analysis_call_graph_edges_duration_ms: 0,
            analysis_rules_duration_ms: 0,
            class_count: 0,
            artifact_count: 0,
            classpath_class_count: 0,
        });
        let sarif = build_sarif(
            None,
            Vec::new(),
            invocation,
            Vec::new(),
            Vec::new(),
            Some("inspequte/./main".to_string()),
        );
        let value = serde_json::to_value(&sarif).expect("serialize SARIF");

        assert_eq!(
            value["runs"][0]["automationDetails"]["id"],
            "inspequte/./main"
        );
    }

    #[test]
    fn sarif_callgraph_snapshot() {
        let temp_dir = make_temp_test_dir();
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let class_a = build_class_a();
        let class_b = build_class_b();
        fs::write(temp_dir.join("A.class"), class_a).expect("write A.class");
        fs::write(temp_dir.join("B.class"), class_b).expect("write B.class");

        let scan = scan_inputs(&[temp_dir.clone()], &[], None).expect("scan classes");
        let artifacts = scan.artifacts.clone();
        let context = build_context(scan.classes.clone(), &artifacts);
        let engine = Engine::new_with_allowed_rule_ids(None).expect("build engine");
        let analysis = engine.analyze(context).expect("analysis");
        let invocation = Invocation::builder()
            .execution_successful(true)
            .arguments(Vec::<String>::new())
            .build();
        let artifacts = normalize_artifacts(artifacts);
        let sarif = build_sarif(
            None,
            artifacts,
            invocation,
            analysis.rules,
            analysis.results,
            None,
        );
        let mut actual_value = serde_json::to_value(&sarif).expect("serialize SARIF");
        normalize_sarif_for_snapshot(&mut actual_value);
        let actual = serde_json::to_string_pretty(&actual_value).expect("serialize SARIF");
        let snapshot_path = snapshot_path("callgraph.sarif");

        if std::env::var("INSPEQUTE_UPDATE_SNAPSHOTS").is_ok() {
            fs::create_dir_all(snapshot_path.parent().expect("snapshot parent"))
                .expect("create snapshot dir");
            let mut file = fs::File::create(&snapshot_path).expect("create snapshot");
            file.write_all(actual.as_bytes()).expect("write snapshot");
            file.write_all(b"\n").expect("write snapshot newline");
        }

        let expected = fs::read_to_string(&snapshot_path).expect("read snapshot");
        let mut expected_value = serde_json::from_str(&expected).expect("deserialize snapshot");
        normalize_sarif_for_snapshot(&mut expected_value);
        let expected = serde_json::to_string_pretty(&expected_value).expect("serialize snapshot");
        assert_eq!(actual.trim_end(), expected.trim_end());

        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    fn normalize_sarif_for_snapshot(value: &mut serde_json::Value) {
        let Some(driver) = value.pointer_mut("/runs/0/tool/driver") else {
            return;
        };
        let Some(driver) = driver.as_object_mut() else {
            return;
        };
        driver.insert(
            "semanticVersion".to_string(),
            serde_json::Value::String("0.0.0".to_string()),
        );
    }

    fn snapshot_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("snapshots")
            .join(name)
    }

    fn build_class_a() -> Vec<u8> {
        let mut builder = ClassFileBuilder::new("A", "java/lang/Object");
        let object_init = builder.add_method_ref("java/lang/Object", "<init>", "()V");
        let b_class = builder.add_class("B");
        let b_init = builder.add_method_ref("B", "<init>", "()V");
        let b_bar = builder.add_method_ref("B", "bar", "()V");

        let init_code = vec![0x2a, 0xb7, high(object_init), low(object_init), 0xb1];
        builder.add_method("<init>", "()V", init_code, 1, 1);

        let foo_code = vec![
            0xbb,
            high(b_class),
            low(b_class),
            0x59,
            0xb7,
            high(b_init),
            low(b_init),
            0xb6,
            high(b_bar),
            low(b_bar),
            0xb1,
        ];
        builder.add_method("foo", "()V", foo_code, 2, 1);

        builder.finish()
    }

    fn build_class_b() -> Vec<u8> {
        let mut builder = ClassFileBuilder::new("B", "java/lang/Object");
        let object_init = builder.add_method_ref("java/lang/Object", "<init>", "()V");
        let init_code = vec![0x2a, 0xb7, high(object_init), low(object_init), 0xb1];
        builder.add_method("<init>", "()V", init_code, 1, 1);
        let bar_code = vec![0xb1];
        builder.add_method("bar", "()V", bar_code, 0, 1);
        builder.finish()
    }

    /// Minimal class file writer for snapshot testing.
    struct ClassFileBuilder {
        cp: Vec<CpEntry>,
        this_class: u16,
        super_class: u16,
        methods: Vec<MethodSpec>,
        code_index: u16,
    }

    impl ClassFileBuilder {
        fn new(class_name: &str, super_name: &str) -> Self {
            let mut builder = Self {
                cp: Vec::new(),
                this_class: 0,
                super_class: 0,
                methods: Vec::new(),
                code_index: 0,
            };
            builder.code_index = builder.add_utf8("Code");
            builder.this_class = builder.add_class(class_name);
            builder.super_class = builder.add_class(super_name);
            builder
        }

        fn add_utf8(&mut self, value: &str) -> u16 {
            self.cp.push(CpEntry::Utf8(value.to_string()));
            self.cp.len() as u16
        }

        fn add_class(&mut self, name: &str) -> u16 {
            let name_index = self.add_utf8(name);
            self.cp.push(CpEntry::Class(name_index));
            self.cp.len() as u16
        }

        fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
            let name_index = self.add_utf8(name);
            let descriptor_index = self.add_utf8(descriptor);
            self.cp
                .push(CpEntry::NameAndType(name_index, descriptor_index));
            self.cp.len() as u16
        }

        fn add_method_ref(&mut self, class: &str, name: &str, descriptor: &str) -> u16 {
            let class_index = self.add_class(class);
            let name_and_type = self.add_name_and_type(name, descriptor);
            self.cp.push(CpEntry::MethodRef(class_index, name_and_type));
            self.cp.len() as u16
        }

        fn add_method(
            &mut self,
            name: &str,
            descriptor: &str,
            code: Vec<u8>,
            max_stack: u16,
            max_locals: u16,
        ) {
            let name_index = self.add_utf8(name);
            let descriptor_index = self.add_utf8(descriptor);
            self.methods.push(MethodSpec {
                name_index,
                descriptor_index,
                code,
                max_stack,
                max_locals,
            });
        }

        fn finish(self) -> Vec<u8> {
            let mut bytes = Vec::new();
            write_u32(&mut bytes, 0xCAFEBABE);
            write_u16(&mut bytes, 0);
            write_u16(&mut bytes, 52);
            write_u16(&mut bytes, (self.cp.len() + 1) as u16);
            for entry in &self.cp {
                entry.write(&mut bytes);
            }
            write_u16(&mut bytes, 0x0021);
            write_u16(&mut bytes, self.this_class);
            write_u16(&mut bytes, self.super_class);
            write_u16(&mut bytes, 0);
            write_u16(&mut bytes, 0);
            write_u16(&mut bytes, self.methods.len() as u16);
            for method in &self.methods {
                write_u16(&mut bytes, 0x0001);
                write_u16(&mut bytes, method.name_index);
                write_u16(&mut bytes, method.descriptor_index);
                write_u16(&mut bytes, 1);
                write_u16(&mut bytes, self.code_index);
                let attr_len = 12 + method.code.len() as u32;
                write_u32(&mut bytes, attr_len);
                write_u16(&mut bytes, method.max_stack);
                write_u16(&mut bytes, method.max_locals);
                write_u32(&mut bytes, method.code.len() as u32);
                bytes.extend_from_slice(&method.code);
                write_u16(&mut bytes, 0);
                write_u16(&mut bytes, 0);
            }
            write_u16(&mut bytes, 0);
            bytes
        }
    }

    /// Method definition for generated class files.
    struct MethodSpec {
        name_index: u16,
        descriptor_index: u16,
        code: Vec<u8>,
        max_stack: u16,
        max_locals: u16,
    }

    /// Constant pool entries needed by snapshot class files.
    enum CpEntry {
        Utf8(String),
        Class(u16),
        NameAndType(u16, u16),
        MethodRef(u16, u16),
    }

    impl CpEntry {
        fn write(&self, bytes: &mut Vec<u8>) {
            match self {
                CpEntry::Utf8(value) => {
                    bytes.push(1);
                    write_u16(bytes, value.len() as u16);
                    bytes.extend_from_slice(value.as_bytes());
                }
                CpEntry::Class(name_index) => {
                    bytes.push(7);
                    write_u16(bytes, *name_index);
                }
                CpEntry::NameAndType(name_index, descriptor_index) => {
                    bytes.push(12);
                    write_u16(bytes, *name_index);
                    write_u16(bytes, *descriptor_index);
                }
                CpEntry::MethodRef(class_index, name_and_type) => {
                    bytes.push(10);
                    write_u16(bytes, *class_index);
                    write_u16(bytes, *name_and_type);
                }
            }
        }
    }

    fn write_u16(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn write_u32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_be_bytes());
    }

    fn high(value: u16) -> u8 {
        (value >> 8) as u8
    }

    fn low(value: u16) -> u8 {
        (value & 0xff) as u8
    }

    fn normalize_artifacts(
        artifacts: Vec<serde_sarif::sarif::Artifact>,
    ) -> Vec<serde_sarif::sarif::Artifact> {
        artifacts
            .into_iter()
            .map(|mut artifact| {
                if let Some(location) = artifact.location.as_mut() {
                    if let Some(uri) = &location.uri {
                        if let Some(name) = artifact_basename(uri) {
                            location.uri = Some(name);
                        }
                    }
                }
                artifact
            })
            .collect()
    }

    fn artifact_basename(uri: &str) -> Option<String> {
        if let Some(rest) = uri.strip_prefix("jar:") {
            let entry = rest.split("!/").nth(1)?;
            return Some(
                PathBuf::from(entry)
                    .file_name()?
                    .to_string_lossy()
                    .to_string(),
            );
        }
        if let Some(rest) = uri.strip_prefix("file://") {
            return Some(
                PathBuf::from(rest)
                    .file_name()?
                    .to_string_lossy()
                    .to_string(),
            );
        }
        PathBuf::from(uri)
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
    }
}
