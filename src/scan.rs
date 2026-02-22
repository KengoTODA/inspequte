use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::io::{Cursor, Read, Seek};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result};
use jclassfile::class_file;
use jclassfile::constant_pool::ConstantPool;
use jclassfile::fields::FieldFlags;
use jclassfile::methods::MethodFlags;
use jdescriptor::{MethodDescriptor, TypeDescriptor};
use serde_json::Value;
use serde_sarif::sarif::{Artifact, ArtifactLocation, ArtifactRoles};
use zip::ZipArchive;

use opentelemetry::Context as OtelContext;
use opentelemetry::KeyValue;
use rayon::prelude::*;

use crate::cfg::build_cfg;
use crate::descriptor::method_param_count;
use crate::ir::{
    CallKind, CallSite, Class, ClassTypeUse, ExceptionHandler, Field, FieldAccess, Instruction,
    InstructionKind, LineNumber, LocalVariableType, Method, MethodAccess, MethodNullness,
    MethodTypeUse, Nullness, TypeParameterUse, TypeUse, TypeUseKind,
};
use crate::opcodes;
use crate::telemetry::Telemetry;

/// Snapshot of parsed artifacts, classes, and counts for a scan.
pub(crate) struct ScanOutput {
    pub(crate) artifacts: Vec<Artifact>,
    pub(crate) class_count: usize,
    pub(crate) classes: Vec<Class>,
}

pub(crate) fn scan_inputs(
    input: &[PathBuf],
    classpath: &[PathBuf],
    telemetry: Option<&Telemetry>,
) -> Result<ScanOutput> {
    // Keep deterministic ordering by sorting classpath entries and directory listings.
    let mut classpath_entries = classpath.to_vec();
    classpath_entries.sort_by_key(|a| path_key(a));

    for entry in input {
        if is_jar_path(entry) {
            classpath_entries.extend(manifest_classpath(entry)?);
        }
    }

    let expanded = expand_classpath(classpath_entries)?;
    let mut targets = Vec::with_capacity(expanded.len() + input.len());
    for (index, entry) in input.iter().enumerate() {
        targets.push(ScanTarget {
            index,
            path: entry.to_path_buf(),
            is_input: true,
        });
    }
    let classpath_offset = input.len();
    for (offset, entry) in expanded.into_iter().enumerate() {
        if input.iter().any(|input| input == &entry) {
            continue;
        }
        targets.push(ScanTarget {
            index: classpath_offset + offset,
            path: entry,
            is_input: false,
        });
    }

    let parent_cx = OtelContext::current();
    let mut results = targets
        .par_iter()
        .map(|target| {
            let _guard = telemetry.map(|_| parent_cx.clone().attach());
            let mut artifacts = Vec::new();
            let mut class_count = 0;
            let mut classes = Vec::new();
            scan_path(
                &target.path,
                target.is_input,
                true,
                telemetry,
                &mut artifacts,
                &mut class_count,
                &mut classes,
            )?;
            Ok((
                target.index,
                ScanOutput {
                    artifacts,
                    class_count,
                    classes,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    results.sort_by_key(|(index, _)| *index);

    let mut artifacts = Vec::new();
    let mut class_count = 0;
    let mut classes = Vec::new();
    for (_, mut output) in results {
        let offset = artifacts.len() as i64;
        for mut artifact in output.artifacts.drain(..) {
            artifact.parent_index = artifact.parent_index.map(|parent| parent + offset);
            artifacts.push(artifact);
        }
        for mut class in output.classes.drain(..) {
            if class.artifact_index >= 0 {
                class.artifact_index += offset;
            }
            classes.push(class);
        }
        class_count += output.class_count;
    }

    Ok(ScanOutput {
        artifacts,
        class_count,
        classes,
    })
}

struct ScanTarget {
    index: usize,
    path: PathBuf,
    is_input: bool,
}

fn scan_path(
    path: &Path,
    is_input: bool,
    strict: bool,
    telemetry: Option<&Telemetry>,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    if path.is_dir() {
        scan_dir(path, is_input, telemetry, artifacts, class_count, classes)?;
        return Ok(());
    }

    let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
    let roles = if is_input {
        Some(vec![
            serde_json::to_value(ArtifactRoles::AnalysisTarget).expect("serialize artifact role"),
        ])
    } else {
        None
    };

    match extension {
        "class" => scan_class_file(path, roles, telemetry, artifacts, class_count, classes),
        "jar" => scan_jar_file(path, roles, telemetry, artifacts, class_count, classes),
        _ => {
            if strict {
                anyhow::bail!("unsupported input file: {}", path.display())
            } else {
                Ok(())
            }
        }
    }
}

fn scan_dir(
    path: &Path,
    is_input: bool,
    telemetry: Option<&Telemetry>,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    let mut entries = Vec::new();
    for entry in fs::read_dir(path)
        .with_context(|| format!("failed to read directory {}", path.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry under {}", path.display()))?;
        entries.push(entry.path());
    }

    entries.sort_by_key(|a | path_key(a));

    for entry in entries {
        if entry.is_dir() {
            scan_dir(&entry, is_input, telemetry, artifacts, class_count, classes)?;
        } else {
            scan_path(
                &entry,
                is_input,
                false,
                telemetry,
                artifacts,
                class_count,
                classes,
            )?;
        }
    }

    Ok(())
}

fn scan_class_file(
    path: &Path,
    roles: Option<Vec<Value>>,
    telemetry: Option<&Telemetry>,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    let (data, parsed) = match telemetry {
        Some(telemetry) => {
            let span_attributes = [KeyValue::new(
                "inspequte.class_path",
                path.display().to_string(),
            )];
            telemetry.in_span(
                "scan.class",
                &span_attributes,
                || -> Result<(Vec<u8>, ParsedClass)> {
                    let data = fs::read(path)
                        .with_context(|| format!("failed to read {}", path.display()))?;
                    let parsed = parse_class_bytes(&data)
                        .with_context(|| format!("failed to parse {}", path.display()))?;
                    Ok((data, parsed))
                },
            )?
        }
        None => {
            let data =
                fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
            let parsed = parse_class_bytes(&data)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            (data, parsed)
        }
    };
    *class_count += 1;

    let artifact_index = if roles.is_some() {
        push_path_artifact(path, roles, data.len() as u64, None, artifacts)?
    } else {
        -1
    };
    classes.push(Class {
        name: parsed.name,
        source_file: parsed.source_file,
        super_name: parsed.super_name,
        interfaces: parsed.interfaces,
        type_parameters: parsed.type_parameters,
        referenced_classes: parsed.referenced_classes,
        fields: parsed.fields,
        methods: parsed.methods,
        artifact_index,
        is_record: parsed.is_record,
    });
    Ok(())
}

fn scan_jar_file(
    path: &Path,
    roles: Option<Vec<Value>>,
    telemetry: Option<&Telemetry>,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    let jar_span_attributes = [KeyValue::new(
        "inspequte.jar_path",
        path.display().to_string(),
    )];
    let result = match telemetry {
        Some(telemetry) => telemetry.in_span("scan.jar", &jar_span_attributes, || {
            scan_jar_file_inner(
                path,
                roles,
                Some(telemetry),
                artifacts,
                class_count,
                classes,
            )
        }),
        None => scan_jar_file_inner(path, roles, None, artifacts, class_count, classes),
    };
    result
}

fn scan_jar_file_inner(
    path: &Path,
    roles: Option<Vec<Value>>,
    telemetry: Option<&Telemetry>,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    let parent_cx = OtelContext::current();
    let jar_path = path.display().to_string();
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read {}", path.display()))?;

    let jar_len = fs::metadata(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .len();
    let jar_index = push_path_artifact(path, roles, jar_len, None, artifacts)?;
    let jar_uri = path_to_uri(path);
    let entries = jar_entries(&jar_path, &mut archive)?;
    let class_entry_bytes =
        read_jar_entries_bytes(&mut archive, &entries.class_entries, &jar_path)?;
    parse_jar_classes(
        &jar_path,
        &jar_path,
        class_entry_bytes,
        jar_index,
        telemetry,
        Some(&parent_cx),
        class_count,
        classes,
    )?;
    scan_nested_jars(
        &mut archive,
        &jar_path,
        &jar_uri,
        jar_index,
        entries.jar_entries,
        telemetry,
        &parent_cx,
        artifacts,
        class_count,
        classes,
    )?;

    Ok(())
}

/// Classified entries inside a JAR archive.
struct JarEntries {
    class_entries: Vec<String>,
    jar_entries: Vec<String>,
}

fn jar_entries<R: Read + Seek>(
    jar_display: &str,
    archive: &mut ZipArchive<R>,
) -> Result<JarEntries> {
    let mut class_entries = Vec::new();
    let mut jar_entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read {}", jar_display))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        // TODO: Handle multi-release entries under META-INF/versions/ in a future release.
        let is_class = name.ends_with(".class")
            && !name.ends_with("module-info.class")
            && !name.starts_with("META-INF/versions/");
        let is_jar = name.ends_with(".jar");
        if is_class {
            class_entries.push(name.clone());
        }
        if is_jar {
            jar_entries.push(name);
        }
    }

    class_entries.sort();
    jar_entries.sort();
    Ok(JarEntries {
        class_entries,
        jar_entries,
    })
}

fn read_jar_entries_bytes<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    entry_names: &[String],
    jar_display: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut entries = Vec::with_capacity(entry_names.len());
    for name in entry_names {
        let mut entry = archive
            .by_name(name)
            .with_context(|| format!("failed to read {}:{}", jar_display, name))?;
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .with_context(|| format!("failed to read {}:{}", jar_display, name))?;
        entries.push((name.clone(), data));
    }
    Ok(entries)
}

fn parse_jar_classes(
    jar_display: &str,
    jar_path_attribute: &str,
    entries: Vec<(String, Vec<u8>)>,
    jar_index: i64,
    telemetry: Option<&Telemetry>,
    parent_cx: Option<&OtelContext>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    let mut parsed = entries
        .par_iter()
        .map(|(name, data)| match telemetry {
            Some(telemetry) => {
                let class_span_attributes = [
                    KeyValue::new("inspequte.jar_path", jar_path_attribute.to_string()),
                    KeyValue::new("inspequte.jar_entry", name.clone()),
                ];
                let parse = || {
                    parse_class_bytes(data)
                        .with_context(|| format!("failed to parse {}:{}", jar_display, name))
                };
                match parent_cx {
                    Some(parent_cx) => telemetry
                        .in_span_with_parent("scan.class", &class_span_attributes, parent_cx, parse)
                        .map(|parsed| (name.clone(), parsed)),
                    None => telemetry
                        .in_span("scan.class", &class_span_attributes, parse)
                        .map(|parsed| (name.clone(), parsed)),
                }
            }
            None => parse_class_bytes(data)
                .with_context(|| format!("failed to parse {}:{}", jar_display, name))
                .map(|parsed| (name.clone(), parsed)),
        })
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    parsed.sort_by(|a, b| a.0.cmp(&b.0));
    *class_count += parsed.len();

    for (_, parsed) in parsed {
        classes.push(Class {
            name: parsed.name,
            source_file: parsed.source_file,
            super_name: parsed.super_name,
            interfaces: parsed.interfaces,
            type_parameters: parsed.type_parameters,
            referenced_classes: parsed.referenced_classes,
            fields: parsed.fields,
            methods: parsed.methods,
            artifact_index: jar_index,
            is_record: parsed.is_record,
        });
    }

    Ok(())
}

fn scan_nested_jars(
    archive: &mut ZipArchive<fs::File>,
    jar_display: &str,
    jar_uri: &str,
    parent_index: i64,
    jar_entries: Vec<String>,
    telemetry: Option<&Telemetry>,
    parent_cx: &OtelContext,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<()> {
    if jar_entries.is_empty() {
        return Ok(());
    }

    let jar_entries_set = jar_entries.iter().cloned().collect::<BTreeSet<String>>();
    let mut queue = VecDeque::from(jar_entries);
    let mut seen = BTreeSet::new();

    while let Some(entry_name) = queue.pop_front() {
        if !seen.insert(entry_name.clone()) {
            continue;
        }
        let jar_bytes = read_jar_entry_bytes(archive, jar_display, &entry_name)?;
        let nested_classpath = scan_nested_jar_entry(
            &entry_name,
            &jar_bytes,
            jar_display,
            jar_uri,
            parent_index,
            telemetry,
            parent_cx,
            artifacts,
            class_count,
            classes,
        )?;
        for nested in nested_classpath {
            if jar_entries_set.contains(&nested) {
                queue.push_back(nested);
            }
        }
    }

    Ok(())
}

fn scan_nested_jar_entry(
    entry_name: &str,
    jar_bytes: &[u8],
    parent_jar_display: &str,
    parent_jar_uri: &str,
    parent_index: i64,
    telemetry: Option<&Telemetry>,
    parent_cx: &OtelContext,
    artifacts: &mut Vec<Artifact>,
    class_count: &mut usize,
    classes: &mut Vec<Class>,
) -> Result<Vec<String>> {
    let jar_display = format!("{parent_jar_display}!/{entry_name}");
    let jar_uri = jar_entry_uri(parent_jar_uri, entry_name);
    let jar_len = jar_bytes.len() as u64;
    let jar_index = push_artifact(
        jar_uri.clone(),
        jar_len,
        Some(parent_index),
        None,
        artifacts,
    );

    let mut archive = ZipArchive::new(Cursor::new(jar_bytes))
        .with_context(|| format!("failed to read {}", jar_display))?;
    let entries = jar_entries(&jar_display, &mut archive)?;
    let class_entry_bytes =
        read_jar_entries_bytes(&mut archive, &entries.class_entries, &jar_display)?;
    parse_jar_classes(
        &jar_display,
        &jar_uri,
        class_entry_bytes,
        jar_index,
        telemetry,
        Some(parent_cx),
        class_count,
        classes,
    )?;

    let classpath_entries = manifest_classpath_entries_from_archive(&mut archive, &jar_display)?;
    Ok(classpath_entries
        .into_iter()
        .map(|entry| resolve_nested_classpath_entry(entry_name, &entry))
        .collect())
}

fn read_jar_entry_bytes(
    archive: &mut ZipArchive<fs::File>,
    jar_display: &str,
    entry_name: &str,
) -> Result<Vec<u8>> {
    let mut entry = archive
        .by_name(entry_name)
        .with_context(|| format!("failed to read {}:{}", jar_display, entry_name))?;
    let mut data = Vec::new();
    entry
        .read_to_end(&mut data)
        .with_context(|| format!("failed to read {}:{}", jar_display, entry_name))?;
    Ok(data)
}

fn jar_entry_uri(parent_uri: &str, entry_name: &str) -> String {
    if parent_uri.starts_with("jar:") {
        format!("{parent_uri}!/{entry_name}")
    } else {
        format!("jar:{parent_uri}!/{entry_name}")
    }
}

fn manifest_classpath_entries_from_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    jar_display: &str,
) -> Result<Vec<String>> {
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read {}", jar_display))?;
        if entry.name() != "META-INF/MANIFEST.MF" {
            continue;
        }
        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}:{}", jar_display, entry.name()))?;
        return Ok(parse_manifest_classpath_entries(&content));
    }
    Ok(Vec::new())
}

fn parse_manifest_classpath_entries(content: &str) -> Vec<String> {
    let mut class_path = None;
    let mut current_key = None;
    let mut current_value = String::new();

    for raw_line in content.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.starts_with(' ') {
            if current_key.is_some() {
                current_value.push_str(&line[1..]);
            }
            continue;
        }

        if let Some(key) = current_key.take() {
            if key == "Class-Path" {
                class_path = Some(current_value.clone());
            }
            current_value.clear();
        }

        if let Some((key, value)) = line.split_once(':') {
            current_key = Some(key.trim().to_string());
            current_value.push_str(value.trim_start());
        }
    }

    if let Some(key) = current_key.take() {
        if key == "Class-Path" {
            class_path = Some(current_value.clone());
        }
    }

    let Some(class_path) = class_path else {
        return Vec::new();
    };

    class_path.split_whitespace().map(str::to_string).collect()
}

fn resolve_nested_classpath_entry(nested_entry_name: &str, classpath_entry: &str) -> String {
    let base_dir = Path::new(nested_entry_name)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let resolved = base_dir.join(classpath_entry);
    normalize_jar_entry_path(&resolved)
}

fn normalize_jar_entry_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Push a path-based artifact and return its index for parent linkage (e.g., JAR entries).
fn push_path_artifact(
    path: &Path,
    roles: Option<Vec<Value>>,
    len: u64,
    parent_index: Option<i64>,
    artifacts: &mut Vec<Artifact>,
) -> Result<i64> {
    let uri = path_to_uri(path);
    Ok(push_artifact(uri, len, parent_index, roles, artifacts))
}

fn push_artifact(
    uri: String,
    len: u64,
    parent_index: Option<i64>,
    roles: Option<Vec<Value>>,
    artifacts: &mut Vec<Artifact>,
) -> i64 {
    let location = ArtifactLocation::builder().uri(uri).build();
    let artifact = match (parent_index, roles) {
        (Some(parent_index), Some(roles)) => Artifact::builder()
            .location(location)
            .length(len as i64)
            .parent_index(parent_index)
            .roles(roles)
            .build(),
        (Some(parent_index), None) => Artifact::builder()
            .location(location)
            .length(len as i64)
            .parent_index(parent_index)
            .build(),
        (None, Some(roles)) => Artifact::builder()
            .location(location)
            .length(len as i64)
            .roles(roles)
            .build(),
        (None, None) => Artifact::builder()
            .location(location)
            .length(len as i64)
            .build(),
    };
    let index = artifacts.len() as i64;
    artifacts.push(artifact);
    index
}

fn path_to_uri(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    format!("file://{}", absolute.to_string_lossy())
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn expand_classpath(initial: Vec<PathBuf>) -> Result<Vec<PathBuf>> {
    let mut queue = VecDeque::new();
    let mut initial_sorted = initial;
    initial_sorted.sort_by_key(|a| path_key(a));
    for entry in initial_sorted {
        queue.push_back(entry);
    }

    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    while let Some(entry) = queue.pop_front() {
        let key = path_key(&entry);
        if !seen.insert(key) {
            continue;
        }
        if !entry.exists() {
            anyhow::bail!("classpath entry not found: {}", entry.display());
        }
        result.push(entry.clone());
        if is_jar_path(&entry) {
            let mut referenced = manifest_classpath(&entry)?;
            referenced.sort_by_key(|a| path_key(a));
            for item in referenced {
                queue.push_back(item);
            }
        }
    }

    Ok(result)
}

fn manifest_classpath(path: &Path) -> Result<Vec<PathBuf>> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut archive =
        ZipArchive::new(file).with_context(|| format!("failed to read {}", path.display()))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if entry.name() != "META-INF/MANIFEST.MF" {
            continue;
        }
        let mut content = String::new();
        entry
            .read_to_string(&mut content)
            .with_context(|| format!("failed to read {}", entry.name()))?;
        return Ok(parse_manifest_classpath(path, &content));
    }

    Ok(Vec::new())
}

fn parse_manifest_classpath(jar_path: &Path, content: &str) -> Vec<PathBuf> {
    let base_dir = jar_path.parent().unwrap_or_else(|| Path::new(""));
    parse_manifest_classpath_entries(content)
        .into_iter()
        .map(|entry| {
            let entry_path = PathBuf::from(entry);
            if entry_path.is_absolute() {
                entry_path
            } else {
                base_dir.join(entry_path)
            }
        })
        .collect()
}

fn is_jar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jar"))
        .unwrap_or(false)
}

/// Parsed class data extracted from class file bytes.
struct ParsedClass {
    name: String,
    source_file: Option<String>,
    super_name: Option<String>,
    interfaces: Vec<String>,
    type_parameters: Vec<TypeParameterUse>,
    referenced_classes: Vec<String>,
    fields: Vec<crate::ir::Field>,
    methods: Vec<Method>,
    is_record: bool,
}

fn parse_class_bytes(data: &[u8]) -> Result<ParsedClass> {
    let class_file = match class_file::parse(data) {
        Ok(parsed) => parsed,
        Err(err) => {
            let message = format!("{err}");
            if message.contains("unmatched attribute") {
                return parse_class_bytes_minimal(data).context("failed to parse class file bytes");
            }
            return Err(err).context("failed to parse class file bytes");
        }
    };
    let constant_pool = class_file.constant_pool();
    let class_name =
        resolve_class_name(constant_pool, class_file.this_class()).context("resolve class name")?;
    let source_file =
        parse_source_file(class_file.attributes(), constant_pool).context("parse source file")?;
    let super_name = if class_file.super_class() == 0 {
        None
    } else {
        Some(
            resolve_class_name(constant_pool, class_file.super_class())
                .context("resolve super class name")?,
        )
    };
    let mut interfaces = Vec::new();
    for interface in class_file.interfaces() {
        interfaces
            .push(resolve_class_name(constant_pool, *interface).context("resolve interface name")?);
    }

    let mut referenced = std::collections::BTreeSet::new();
    for entry in constant_pool {
        if let ConstantPool::Class { name_index } = entry {
            let name = resolve_utf8(constant_pool, *name_index)
                .context("resolve referenced class name")?;
            if let Some(normalized) = normalize_class_name(&name) {
                referenced.insert(normalized);
            }
        }
    }
    referenced.remove(&class_name);

    let is_record = class_file
        .attributes()
        .iter()
        .any(|attr| matches!(attr, jclassfile::attributes::Attribute::Record { .. }));
    let default_nullness = parse_default_nullness(class_file.attributes(), constant_pool)
        .context("parse class nullness")?;
    let class_signature =
        parse_signature(class_file.attributes(), constant_pool).context("parse class signature")?;
    let type_parameters = parse_class_type_parameters(class_signature.as_deref(), default_nullness)
        .context("parse class type parameters")?;
    let fields = parse_fields(constant_pool, class_file.fields(), default_nullness)
        .context("parse fields")?;
    let methods = parse_methods(constant_pool, class_file.methods(), default_nullness)
        .context("parse method bytecode")?;

    Ok(ParsedClass {
        name: class_name,
        source_file,
        super_name,
        interfaces,
        type_parameters,
        referenced_classes: referenced.into_iter().collect(),
        fields,
        methods,
        is_record,
    })
}

fn resolve_class_name(constant_pool: &[ConstantPool], class_index: u16) -> Result<String> {
    let entry = constant_pool
        .get(class_index as usize)
        .context("missing class entry")?;
    match entry {
        ConstantPool::Class { name_index } => resolve_utf8(constant_pool, *name_index),
        _ => anyhow::bail!("unexpected class entry"),
    }
}

fn resolve_utf8(constant_pool: &[ConstantPool], index: u16) -> Result<String> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing utf8 entry")?;
    match entry {
        ConstantPool::Utf8 { value } => Ok(value.clone()),
        _ => anyhow::bail!("unexpected utf8 entry"),
    }
}

fn normalize_class_name(raw: &str) -> Option<String> {
    if !raw.starts_with('[') {
        return Some(raw.to_string());
    }
    let mut slice = raw;
    while let Some(rest) = slice.strip_prefix('[') {
        slice = rest;
    }
    if let Some(class_name) = slice.strip_prefix('L').and_then(|s| s.strip_suffix(';')) {
        return Some(class_name.to_string());
    }
    None
}

fn parse_class_bytes_minimal(data: &[u8]) -> Result<ParsedClass> {
    let mut offset = 0usize;
    let magic = read_u32_class(data, &mut offset)?;
    if magic != 0xCAFEBABE {
        anyhow::bail!("invalid class file magic");
    }
    let _minor = read_u16_class(data, &mut offset)?;
    let _major = read_u16_class(data, &mut offset)?;
    let (cp_entries, class_entries) = parse_constant_pool_minimal(data, &mut offset)?;
    let _access_flags = read_u16_class(data, &mut offset)?;
    let this_class = read_u16_class(data, &mut offset)?;
    let super_class = read_u16_class(data, &mut offset)?;

    let class_name = resolve_class_name_minimal(&cp_entries, &class_entries, this_class)
        .context("resolve class name")?;
    let super_name = if super_class == 0 {
        None
    } else {
        Some(
            resolve_class_name_minimal(&cp_entries, &class_entries, super_class)
                .context("resolve super class name")?,
        )
    };

    let interfaces = parse_interfaces_minimal(data, &mut offset, &cp_entries, &class_entries)?;
    skip_fields(data, &mut offset)?;
    skip_methods(data, &mut offset)?;
    skip_attributes(data, &mut offset)?;

    let mut referenced = std::collections::BTreeSet::new();
    for (index, name_index) in class_entries.iter().enumerate() {
        if name_index.is_none() {
            continue;
        }
        let name = resolve_class_name_minimal(&cp_entries, &class_entries, index as u16)?;
        if let Some(normalized) = normalize_class_name(&name) {
            referenced.insert(normalized);
        }
    }
    referenced.remove(&class_name);

    Ok(ParsedClass {
        name: class_name,
        source_file: None,
        super_name,
        interfaces,
        type_parameters: Vec::new(),
        referenced_classes: referenced.into_iter().collect(),
        fields: Vec::new(),
        methods: Vec::new(),
        is_record: false,
    })
}

#[derive(Clone)]
enum CpEntryMin {
    Utf8(String),
    Other,
}

fn parse_constant_pool_minimal(
    data: &[u8],
    offset: &mut usize,
) -> Result<(Vec<CpEntryMin>, Vec<Option<u16>>)> {
    let count = read_u16_class(data, offset)?;
    let mut entries = Vec::with_capacity(count as usize);
    entries.push(CpEntryMin::Other);
    let mut class_entries = vec![None; count as usize];
    let mut index = 1u16;
    while index < count {
        let tag = read_u8_class(data, offset)?;
        match tag {
            1 => {
                let len = read_u16_class(data, offset)? as usize;
                let bytes = read_bytes_class(data, offset, len)?;
                let value = String::from_utf8_lossy(bytes).to_string();
                entries.push(CpEntryMin::Utf8(value));
            }
            7 => {
                let name_index = read_u16_class(data, offset)?;
                entries.push(CpEntryMin::Other);
                class_entries[index as usize] = Some(name_index);
            }
            3 | 4 => {
                skip_class_bytes(data, offset, 4)?;
                entries.push(CpEntryMin::Other);
            }
            5 | 6 => {
                skip_class_bytes(data, offset, 8)?;
                entries.push(CpEntryMin::Other);
                entries.push(CpEntryMin::Other);
                index += 1;
            }
            8 => {
                skip_class_bytes(data, offset, 2)?;
                entries.push(CpEntryMin::Other);
            }
            9 | 10 | 11 | 12 | 18 => {
                skip_class_bytes(data, offset, 4)?;
                entries.push(CpEntryMin::Other);
            }
            15 => {
                skip_class_bytes(data, offset, 3)?;
                entries.push(CpEntryMin::Other);
            }
            16 | 19 | 20 => {
                skip_class_bytes(data, offset, 2)?;
                entries.push(CpEntryMin::Other);
            }
            17 => {
                skip_class_bytes(data, offset, 4)?;
                entries.push(CpEntryMin::Other);
            }
            _ => anyhow::bail!("unsupported constant pool tag: {}", tag),
        }
        index += 1;
    }
    Ok((entries, class_entries))
}

fn resolve_class_name_minimal(
    entries: &[CpEntryMin],
    class_entries: &[Option<u16>],
    class_index: u16,
) -> Result<String> {
    let entry = class_entries
        .get(class_index as usize)
        .context("missing class entry")?;
    let name_index = entry.context("missing class name index")?;
    match entries.get(name_index as usize) {
        Some(CpEntryMin::Utf8(value)) => Ok(value.clone()),
        _ => anyhow::bail!("missing utf8 entry for class name"),
    }
}

fn parse_interfaces_minimal(
    data: &[u8],
    offset: &mut usize,
    entries: &[CpEntryMin],
    class_entries: &[Option<u16>],
) -> Result<Vec<String>> {
    let count = read_u16_class(data, offset)? as usize;
    let mut interfaces = Vec::with_capacity(count);
    for _ in 0..count {
        let index = read_u16_class(data, offset)?;
        interfaces.push(resolve_class_name_minimal(entries, class_entries, index)?);
    }
    Ok(interfaces)
}

fn skip_fields(data: &[u8], offset: &mut usize) -> Result<()> {
    let count = read_u16_class(data, offset)?;
    for _ in 0..count {
        skip_class_bytes(data, offset, 6)?;
        skip_attributes(data, offset)?;
    }
    Ok(())
}

fn skip_methods(data: &[u8], offset: &mut usize) -> Result<()> {
    let count = read_u16_class(data, offset)?;
    for _ in 0..count {
        skip_class_bytes(data, offset, 6)?;
        skip_attributes(data, offset)?;
    }
    Ok(())
}

fn skip_attributes(data: &[u8], offset: &mut usize) -> Result<()> {
    let count = read_u16_class(data, offset)?;
    for _ in 0..count {
        skip_class_bytes(data, offset, 2)?;
        let length = read_u32_class(data, offset)? as usize;
        skip_class_bytes(data, offset, length)?;
    }
    Ok(())
}

fn read_u8_class(data: &[u8], offset: &mut usize) -> Result<u8> {
    let byte = *data.get(*offset).context("class file out of bounds")?;
    *offset += 1;
    Ok(byte)
}

fn read_u16_class(data: &[u8], offset: &mut usize) -> Result<u16> {
    let bytes = read_bytes_class(data, offset, 2)?;
    Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
}

fn read_u32_class(data: &[u8], offset: &mut usize) -> Result<u32> {
    let bytes = read_bytes_class(data, offset, 4)?;
    Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_bytes_class<'a>(data: &'a [u8], offset: &mut usize, len: usize) -> Result<&'a [u8]> {
    let start = *offset;
    let end = start + len;
    let slice = data.get(start..end).context("class file out of bounds")?;
    *offset = end;
    Ok(slice)
}

fn skip_class_bytes(data: &[u8], offset: &mut usize, len: usize) -> Result<()> {
    read_bytes_class(data, offset, len)?;
    Ok(())
}

fn parse_fields(
    constant_pool: &[ConstantPool],
    fields: &[jclassfile::fields::FieldInfo],
    default_nullness: DefaultNullness,
) -> Result<Vec<Field>> {
    let mut parsed = Vec::new();
    for field in fields {
        let name = resolve_utf8(constant_pool, field.name_index()).context("resolve field name")?;
        let descriptor = resolve_utf8(constant_pool, field.descriptor_index())
            .context("resolve field descriptor")?;
        let signature =
            parse_signature(field.attributes(), constant_pool).context("parse field signature")?;
        let type_use = parse_field_type_use(
            constant_pool,
            field.attributes(),
            signature.as_deref(),
            &descriptor,
            default_nullness,
        )
        .context("parse field type-use")?;
        let access_flags = field.access_flags();
        let access = FieldAccess {
            is_static: access_flags.contains(FieldFlags::ACC_STATIC),
            is_private: access_flags.contains(FieldFlags::ACC_PRIVATE),
            is_final: access_flags.contains(FieldFlags::ACC_FINAL),
        };
        parsed.push(Field {
            name,
            descriptor,
            signature,
            type_use,
            access,
        });
    }
    Ok(parsed)
}

fn parse_methods(
    constant_pool: &[ConstantPool],
    methods: &[jclassfile::methods::MethodInfo],
    default_nullness: DefaultNullness,
) -> Result<Vec<Method>> {
    let mut parsed = Vec::new();
    for method in methods {
        let name =
            resolve_utf8(constant_pool, method.name_index()).context("resolve method name")?;
        let descriptor = resolve_utf8(constant_pool, method.descriptor_index())
            .context("resolve method descriptor")?;
        let signature = parse_signature(method.attributes(), constant_pool)
            .context("parse method signature")?;
        let access_flags = method.access_flags();
        let access = MethodAccess {
            is_public: access_flags.contains(MethodFlags::ACC_PUBLIC),
            is_static: access_flags.contains(MethodFlags::ACC_STATIC),
            is_abstract: access_flags.contains(MethodFlags::ACC_ABSTRACT),
        };
        let nullness = parse_method_nullness(
            constant_pool,
            method.attributes(),
            &descriptor,
            default_nullness,
        )
        .context("parse method nullness")?;
        let type_use = parse_method_type_use(
            constant_pool,
            method.attributes(),
            signature.as_deref(),
            &descriptor,
            default_nullness,
        )
        .context("parse method type-use")?;
        let code = method
            .attributes()
            .iter()
            .find_map(|attribute| match attribute {
                jclassfile::attributes::Attribute::Code {
                    code,
                    exception_table,
                    attributes,
                    ..
                } => Some((code, exception_table, attributes)),
                _ => None,
            });
        let Some((code, exception_table, code_attributes)) = code else {
            continue;
        };
        let line_numbers =
            parse_line_numbers(code_attributes, constant_pool).context("parse line numbers")?;
        let (instructions, calls, string_literals) =
            parse_bytecode(code, constant_pool).context("parse bytecode")?;
        let exception_handlers =
            parse_exception_handlers(exception_table, constant_pool).context("parse handlers")?;
        let local_variable_types =
            parse_local_variable_types(code_attributes, constant_pool, default_nullness)
                .context("parse local variable types")?;
        let handler_offsets = exception_handlers
            .iter()
            .map(|handler| handler.handler_pc)
            .collect::<Vec<_>>();
        let cfg =
            build_cfg(code, &instructions, &handler_offsets).context("build control flow graph")?;
        parsed.push(Method {
            name,
            descriptor,
            signature,
            access,
            nullness,
            type_use,
            bytecode: code.clone(),
            line_numbers,
            cfg,
            calls,
            string_literals,
            exception_handlers,
            local_variable_types,
        });
    }
    Ok(parsed)
}

fn parse_line_numbers(
    attributes: &[jclassfile::attributes::Attribute],
    _constant_pool: &[ConstantPool],
) -> Result<Vec<LineNumber>> {
    let mut entries = Vec::new();
    for attribute in attributes {
        let jclassfile::attributes::Attribute::LineNumberTable { line_number_table } = attribute
        else {
            continue;
        };
        for record in line_number_table {
            entries.push(LineNumber {
                start_pc: record.start_pc() as u32,
                line: record.line_number() as u32,
            });
        }
    }
    entries.sort_by_key(|entry| entry.start_pc);
    Ok(entries)
}

fn parse_signature(
    attributes: &[jclassfile::attributes::Attribute],
    constant_pool: &[ConstantPool],
) -> Result<Option<String>> {
    for attribute in attributes {
        let jclassfile::attributes::Attribute::Signature { signature_index } = attribute else {
            continue;
        };
        let signature =
            resolve_utf8(constant_pool, *signature_index).context("resolve signature")?;
        return Ok(Some(signature));
    }
    Ok(None)
}

fn parse_source_file(
    attributes: &[jclassfile::attributes::Attribute],
    constant_pool: &[ConstantPool],
) -> Result<Option<String>> {
    for attribute in attributes {
        let jclassfile::attributes::Attribute::SourceFile { sourcefile_index } = attribute else {
            continue;
        };
        let source_file =
            resolve_utf8(constant_pool, *sourcefile_index).context("resolve source file")?;
        return Ok(Some(source_file));
    }
    Ok(None)
}

fn parse_local_variable_types(
    attributes: &[jclassfile::attributes::Attribute],
    constant_pool: &[ConstantPool],
    default_nullness: DefaultNullness,
) -> Result<Vec<LocalVariableType>> {
    let mut locals = Vec::new();
    for attribute in attributes {
        let jclassfile::attributes::Attribute::LocalVariableTypeTable {
            local_variable_type_table,
        } = attribute
        else {
            continue;
        };
        for record in local_variable_type_table {
            let name =
                resolve_utf8(constant_pool, record.name_index()).context("resolve local name")?;
            let signature = resolve_utf8(constant_pool, record.signature_index())
                .context("resolve local signature")?;
            let type_use = Some(parse_type_use_signature(&signature)?);
            locals.push(LocalVariableType {
                name,
                signature,
                type_use,
                index: record.index(),
                start_pc: record.start_pc() as u32,
                length: record.length() as u32,
            });
        }
    }
    apply_local_variable_type_annotations(constant_pool, attributes, &mut locals)?;
    if default_nullness == DefaultNullness::NonNull {
        for local in &mut locals {
            if let Some(ty) = local.type_use.as_mut() {
                apply_default_nullness(ty);
            }
        }
    }
    Ok(locals)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DefaultNullness {
    Inherit,
    Unmarked,
    NonNull,
}

fn parse_default_nullness(
    attributes: &[jclassfile::attributes::Attribute],
    constant_pool: &[ConstantPool],
) -> Result<DefaultNullness> {
    let mut has_marked = false;
    let mut has_unmarked = false;
    for attribute in attributes {
        let jclassfile::attributes::Attribute::RuntimeVisibleAnnotations { annotations, .. } =
            attribute
        else {
            continue;
        };
        for annotation in annotations {
            let name = annotation_class_name(constant_pool, annotation)?;
            match name.as_str() {
                "org/jspecify/annotations/NullMarked" => has_marked = true,
                "org/jspecify/annotations/NullUnmarked" => has_unmarked = true,
                _ => {}
            }
        }
    }
    if has_unmarked {
        return Ok(DefaultNullness::Unmarked);
    }
    if has_marked {
        return Ok(DefaultNullness::NonNull);
    }
    Ok(DefaultNullness::Inherit)
}

fn parse_method_nullness(
    constant_pool: &[ConstantPool],
    attributes: &[jclassfile::attributes::Attribute],
    descriptor: &str,
    class_default: DefaultNullness,
) -> Result<MethodNullness> {
    let param_count = method_param_count(descriptor)?;
    let mut nullness = MethodNullness::unknown(param_count);
    for attribute in attributes {
        let jclassfile::attributes::Attribute::RuntimeVisibleTypeAnnotations { type_annotations } =
            attribute
        else {
            continue;
        };
        for annotation in type_annotations {
            if !annotation.type_path().is_empty() {
                continue;
            }
            let Some(value) = nullness_from_annotation(constant_pool, annotation.annotation())?
            else {
                continue;
            };
            match (annotation.target_type(), annotation.target_info()) {
                (
                    jclassfile::attributes::TargetType::METHOD_RETURN,
                    jclassfile::attributes::TargetInfo::EmptyTarget,
                ) => apply_nullness(&mut nullness.return_nullness, value),
                (
                    jclassfile::attributes::TargetType::METHOD_FORMAL_PARAMETER,
                    jclassfile::attributes::TargetInfo::FormalParameterTarget {
                        formal_parameter_index,
                    },
                ) => {
                    let index = *formal_parameter_index as usize;
                    if let Some(param) = nullness.parameter_nullness.get_mut(index) {
                        apply_nullness(param, value);
                    }
                }
                _ => {}
            }
        }
    }
    let method_default = parse_default_nullness(attributes, constant_pool)?;
    let effective_default = match method_default {
        DefaultNullness::Inherit => class_default,
        value => value,
    };
    if effective_default == DefaultNullness::NonNull {
        let descriptor =
            MethodDescriptor::from_str(descriptor).context("parse method descriptor")?;
        for (index, param) in descriptor.parameter_types().iter().enumerate() {
            if is_reference_type(param) {
                if let Some(param_nullness) = nullness.parameter_nullness.get_mut(index) {
                    if *param_nullness == Nullness::Unknown {
                        *param_nullness = Nullness::NonNull;
                    }
                }
            }
        }
        if is_reference_type(descriptor.return_type())
            && nullness.return_nullness == Nullness::Unknown
        {
            nullness.return_nullness = Nullness::NonNull;
        }
    }
    Ok(nullness)
}

fn parse_method_type_use(
    constant_pool: &[ConstantPool],
    attributes: &[jclassfile::attributes::Attribute],
    signature: Option<&str>,
    descriptor: &str,
    class_default: DefaultNullness,
) -> Result<Option<MethodTypeUse>> {
    let mut type_use = if let Some(signature) = signature {
        parse_method_type_use_signature(signature)?
    } else {
        method_type_use_from_descriptor(descriptor)?
    };
    for attribute in attributes {
        let jclassfile::attributes::Attribute::RuntimeVisibleTypeAnnotations { type_annotations } =
            attribute
        else {
            continue;
        };
        for annotation in type_annotations {
            let Some(value) = nullness_from_annotation(constant_pool, annotation.annotation())?
            else {
                continue;
            };
            match (annotation.target_type(), annotation.target_info()) {
                (
                    jclassfile::attributes::TargetType::METHOD_RETURN,
                    jclassfile::attributes::TargetInfo::EmptyTarget,
                ) => {
                    if let Some(return_type) = type_use.return_type.as_mut() {
                        apply_type_use_annotation(return_type, annotation.type_path(), value);
                    }
                }
                (
                    jclassfile::attributes::TargetType::METHOD_FORMAL_PARAMETER,
                    jclassfile::attributes::TargetInfo::FormalParameterTarget {
                        formal_parameter_index,
                    },
                ) => {
                    let index = *formal_parameter_index as usize;
                    if let Some(param) = type_use.parameters.get_mut(index) {
                        apply_type_use_annotation(param, annotation.type_path(), value);
                    }
                }
                (
                    jclassfile::attributes::TargetType::METHOD_TYPE_PARAMETER_BOUND,
                    jclassfile::attributes::TargetInfo::TypeParameterBoundTarget {
                        type_parameter_index,
                        bound_index,
                    },
                ) => {
                    apply_type_parameter_bound_annotation(
                        &mut type_use.type_parameters,
                        *type_parameter_index as usize,
                        *bound_index as usize,
                        annotation.type_path(),
                        value,
                    );
                }
                _ => {}
            }
        }
    }
    let method_default = parse_default_nullness(attributes, constant_pool)?;
    let effective_default = match method_default {
        DefaultNullness::Inherit => class_default,
        value => value,
    };
    if effective_default == DefaultNullness::NonNull {
        for param in &mut type_use.parameters {
            apply_default_nullness(param);
        }
        for type_param in &mut type_use.type_parameters {
            apply_default_nullness_to_type_parameter(type_param);
        }
        if let Some(return_type) = type_use.return_type.as_mut() {
            apply_default_nullness(return_type);
        }
    }
    Ok(Some(type_use))
}

fn parse_class_type_parameters(
    signature: Option<&str>,
    default_nullness: DefaultNullness,
) -> Result<Vec<TypeParameterUse>> {
    let Some(signature) = signature else {
        return Ok(Vec::new());
    };
    let mut parser = TypeUseSignatureParser::new(signature);
    let mut type_parameters = parser
        .parse_type_parameters()
        .context("parse class type parameters")?;
    if default_nullness == DefaultNullness::NonNull {
        for parameter in &mut type_parameters {
            apply_default_nullness_to_type_parameter(parameter);
        }
    }
    Ok(type_parameters)
}

fn parse_field_type_use(
    constant_pool: &[ConstantPool],
    attributes: &[jclassfile::attributes::Attribute],
    signature: Option<&str>,
    descriptor: &str,
    class_default: DefaultNullness,
) -> Result<Option<TypeUse>> {
    let mut type_use = if let Some(signature) = signature {
        parse_type_use_signature(signature)?
    } else {
        let descriptor = TypeDescriptor::from_str(descriptor).context("parse field descriptor")?;
        type_use_from_descriptor(&descriptor)
    };
    for attribute in attributes {
        let jclassfile::attributes::Attribute::RuntimeVisibleTypeAnnotations { type_annotations } =
            attribute
        else {
            continue;
        };
        for annotation in type_annotations {
            let Some(value) = nullness_from_annotation(constant_pool, annotation.annotation())?
            else {
                continue;
            };
            match (annotation.target_type(), annotation.target_info()) {
                (
                    jclassfile::attributes::TargetType::FIELD,
                    jclassfile::attributes::TargetInfo::EmptyTarget,
                ) => apply_type_use_annotation(&mut type_use, annotation.type_path(), value),
                _ => {}
            }
        }
    }
    if class_default == DefaultNullness::NonNull {
        apply_default_nullness(&mut type_use);
    }
    Ok(Some(type_use))
}

fn apply_local_variable_type_annotations(
    constant_pool: &[ConstantPool],
    attributes: &[jclassfile::attributes::Attribute],
    locals: &mut [LocalVariableType],
) -> Result<()> {
    for attribute in attributes {
        let jclassfile::attributes::Attribute::RuntimeVisibleTypeAnnotations { type_annotations } =
            attribute
        else {
            continue;
        };
        for annotation in type_annotations {
            let Some(value) = nullness_from_annotation(constant_pool, annotation.annotation())?
            else {
                continue;
            };
            match (annotation.target_type(), annotation.target_info()) {
                (
                    jclassfile::attributes::TargetType::LOCAL_VARIABLE,
                    jclassfile::attributes::TargetInfo::LocalvarTarget { table },
                )
                | (
                    jclassfile::attributes::TargetType::RESOURCE_VARIABLE,
                    jclassfile::attributes::TargetInfo::LocalvarTarget { table },
                ) => {
                    for entry in table {
                        for local in locals.iter_mut() {
                            if local.index == entry.index()
                                && local.start_pc == entry.start_pc() as u32
                                && local.length == entry.length() as u32
                            {
                                if let Some(type_use) = local.type_use.as_mut() {
                                    apply_type_use_annotation(
                                        type_use,
                                        annotation.type_path(),
                                        value,
                                    );
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn apply_type_parameter_bound_annotation(
    parameters: &mut [TypeParameterUse],
    parameter_index: usize,
    bound_index: usize,
    path: &[jclassfile::attributes::TypePathEntry],
    value: Nullness,
) {
    let Some(parameter) = parameters.get_mut(parameter_index) else {
        return;
    };
    if bound_index == 0 {
        if let Some(bound) = parameter.class_bound.as_mut() {
            apply_type_use_annotation(bound, path, value);
        }
        return;
    }
    let index = bound_index - 1;
    if let Some(bound) = parameter.interface_bounds.get_mut(index) {
        apply_type_use_annotation(bound, path, value);
    }
}

fn apply_default_nullness_to_type_parameter(parameter: &mut TypeParameterUse) {
    if let Some(bound) = parameter.class_bound.as_mut() {
        apply_default_nullness(bound);
    }
    for bound in &mut parameter.interface_bounds {
        apply_default_nullness(bound);
    }
}

fn parse_method_type_use_signature(signature: &str) -> Result<MethodTypeUse> {
    let mut parser = TypeUseSignatureParser::new(signature);
    let type_parameters = parser
        .parse_type_parameters()
        .context("parse type parameters")?;
    let first = parser
        .peek()
        .context("invalid method signature: expected '('")?;
    if first != b'(' {
        anyhow::bail!("invalid method signature: expected '('");
    }
    parser.bump();
    let mut params = Vec::new();
    while let Some(byte) = parser.peek() {
        if byte == b')' {
            parser.bump();
            break;
        }
        params.push(
            parser
                .parse_type_signature()
                .context("parse parameter type signature")?,
        );
    }
    let return_type = match parser.peek().context("parse return type signature")? {
        b'V' => {
            parser.bump();
            None
        }
        _ => Some(
            parser
                .parse_type_signature()
                .context("parse return type signature")?,
        ),
    };
    Ok(MethodTypeUse {
        type_parameters,
        parameters: params,
        return_type,
    })
}

fn method_type_use_from_descriptor(descriptor: &str) -> Result<MethodTypeUse> {
    let descriptor = MethodDescriptor::from_str(descriptor).context("parse method descriptor")?;
    let parameters = descriptor
        .parameter_types()
        .iter()
        .map(type_use_from_descriptor)
        .collect();
    let return_type = match descriptor.return_type() {
        TypeDescriptor::Void => None,
        ty => Some(type_use_from_descriptor(ty)),
    };
    Ok(MethodTypeUse {
        type_parameters: Vec::new(),
        parameters,
        return_type,
    })
}

fn parse_type_use_signature(signature: &str) -> Result<TypeUse> {
    let mut parser = TypeUseSignatureParser::new(signature);
    parser
        .parse_type_signature()
        .context("parse type signature")
}

fn type_use_from_descriptor(descriptor: &TypeDescriptor) -> TypeUse {
    match descriptor {
        TypeDescriptor::Byte => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('B'),
        },
        TypeDescriptor::Char => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('C'),
        },
        TypeDescriptor::Double => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('D'),
        },
        TypeDescriptor::Float => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('F'),
        },
        TypeDescriptor::Integer => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('I'),
        },
        TypeDescriptor::Long => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('J'),
        },
        TypeDescriptor::Short => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('S'),
        },
        TypeDescriptor::Boolean => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Base('Z'),
        },
        TypeDescriptor::Void => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Void,
        },
        TypeDescriptor::Object(name) => TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::Class(ClassTypeUse {
                name: name.clone(),
                type_arguments: Vec::new(),
                inner: None,
            }),
        },
        TypeDescriptor::Array(component, depth) => {
            let mut current = type_use_from_descriptor(component);
            for _ in 0..*depth {
                current = TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Array(Box::new(current)),
                };
            }
            current
        }
    }
}

fn apply_type_use_annotation(
    target: &mut TypeUse,
    path: &[jclassfile::attributes::TypePathEntry],
    value: Nullness,
) {
    let mut current = target;
    for entry in path {
        match entry.path_kind() {
            0 => {
                let TypeUseKind::Array(component) = &mut current.kind else {
                    return;
                };
                current = component;
            }
            1 => {
                let TypeUseKind::Class(class) = &mut current.kind else {
                    return;
                };
                let Some(inner) = class.inner.as_mut() else {
                    return;
                };
                current = inner.as_mut();
            }
            2 => {
                let TypeUseKind::Wildcard(Some(bound)) = &mut current.kind else {
                    return;
                };
                current = bound;
            }
            3 => {
                let TypeUseKind::Class(class) = &mut current.kind else {
                    return;
                };
                let index = entry.path_index() as usize;
                let Some(arg) = class.type_arguments.get_mut(index) else {
                    return;
                };
                current = arg;
            }
            _ => return,
        }
    }
    apply_nullness(&mut current.nullness, value);
}

fn apply_default_nullness(target: &mut TypeUse) {
    if is_reference_type_use(target) && target.nullness == Nullness::Unknown {
        target.nullness = Nullness::NonNull;
    }
    match &mut target.kind {
        TypeUseKind::Array(component) => apply_default_nullness(component),
        TypeUseKind::Class(class) => apply_default_nullness_to_class_type(class),
        TypeUseKind::Wildcard(Some(bound)) => apply_default_nullness(bound),
        _ => {}
    }
}

fn apply_default_nullness_to_class_type(class: &mut ClassTypeUse) {
    for arg in &mut class.type_arguments {
        apply_default_nullness(arg);
    }
    if let Some(inner) = class.inner.as_mut() {
        apply_default_nullness(inner);
    }
}

fn is_reference_type_use(target: &TypeUse) -> bool {
    matches!(
        target.kind,
        TypeUseKind::Array(_)
            | TypeUseKind::Class(_)
            | TypeUseKind::TypeVar(_)
            | TypeUseKind::Wildcard(_)
    )
}

struct TypeUseSignatureParser<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> TypeUseSignatureParser<'a> {
    fn new(signature: &'a str) -> Self {
        Self {
            bytes: signature.as_bytes(),
            offset: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.offset += 1;
        Some(byte)
    }

    fn parse_type_parameters(&mut self) -> Option<Vec<TypeParameterUse>> {
        if self.peek() != Some(b'<') {
            return Some(Vec::new());
        }
        self.bump();
        let mut params = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b'>' {
                self.bump();
                break;
            }
            let mut name = String::new();
            while let Some(byte) = self.peek() {
                if byte == b':' {
                    break;
                }
                name.push(byte as char);
                self.bump();
            }
            if self.peek()? != b':' {
                return None;
            }
            self.bump();
            let class_bound = if self.peek() != Some(b':') {
                self.parse_reference_type_signature()
            } else {
                None
            };
            let mut interface_bounds = Vec::new();
            while self.peek() == Some(b':') {
                self.bump();
                interface_bounds.push(self.parse_reference_type_signature()?);
            }
            params.push(TypeParameterUse {
                name,
                class_bound,
                interface_bounds,
            });
        }
        Some(params)
    }

    fn parse_type_signature(&mut self) -> Option<TypeUse> {
        match self.peek()? {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => Some(TypeUse {
                nullness: Nullness::Unknown,
                kind: TypeUseKind::Base(self.bump()? as char),
            }),
            b'V' => {
                self.bump();
                Some(TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Void,
                })
            }
            b'[' => {
                self.bump();
                let component = self.parse_type_signature()?;
                Some(TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Array(Box::new(component)),
                })
            }
            b'T' => self.parse_type_variable(),
            b'L' => self.parse_class_type_signature().map(|class| TypeUse {
                nullness: Nullness::Unknown,
                kind: TypeUseKind::Class(class),
            }),
            _ => None,
        }
    }

    fn parse_reference_type_signature(&mut self) -> Option<TypeUse> {
        match self.peek()? {
            b'L' => self.parse_class_type_signature().map(|class| TypeUse {
                nullness: Nullness::Unknown,
                kind: TypeUseKind::Class(class),
            }),
            b'T' => self.parse_type_variable(),
            b'[' => {
                self.bump();
                let component = self.parse_type_signature()?;
                Some(TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Array(Box::new(component)),
                })
            }
            _ => None,
        }
    }

    fn parse_type_variable(&mut self) -> Option<TypeUse> {
        if self.peek()? != b'T' {
            return None;
        }
        self.bump();
        let mut name = String::new();
        while let Some(byte) = self.peek() {
            if byte == b';' {
                self.bump();
                break;
            }
            name.push(byte as char);
            self.bump();
        }
        Some(TypeUse {
            nullness: Nullness::Unknown,
            kind: TypeUseKind::TypeVar(name),
        })
    }

    fn parse_class_type_signature(&mut self) -> Option<ClassTypeUse> {
        if self.peek()? != b'L' {
            return None;
        }
        self.bump();
        let mut name = String::new();
        while let Some(byte) = self.peek() {
            match byte {
                b';' | b'<' | b'.' => break,
                _ => {
                    name.push(byte as char);
                    self.bump();
                }
            }
        }
        let type_arguments = self.parse_type_arguments()?;
        let mut root = ClassTypeUse {
            name,
            type_arguments,
            inner: None,
        };
        let mut cursor = &mut root;
        while let Some(b'.') = self.peek() {
            self.bump();
            let mut inner_name = String::new();
            while let Some(byte) = self.peek() {
                match byte {
                    b';' | b'<' | b'.' => break,
                    _ => {
                        inner_name.push(byte as char);
                        self.bump();
                    }
                }
            }
            let inner_arguments = self.parse_type_arguments()?;
            cursor.inner = Some(Box::new(TypeUse {
                nullness: Nullness::Unknown,
                kind: TypeUseKind::Class(ClassTypeUse {
                    name: inner_name,
                    type_arguments: inner_arguments,
                    inner: None,
                }),
            }));
            let Some(inner) = cursor.inner.as_mut() else {
                return None;
            };
            let TypeUseKind::Class(class) = &mut inner.kind else {
                return None;
            };
            cursor = class;
        }
        if self.peek()? != b';' {
            return None;
        }
        self.bump();
        Some(root)
    }

    fn parse_type_arguments(&mut self) -> Option<Vec<TypeUse>> {
        if self.peek() != Some(b'<') {
            return Some(Vec::new());
        }
        self.bump();
        let mut args = Vec::new();
        while let Some(byte) = self.peek() {
            if byte == b'>' {
                self.bump();
                break;
            }
            args.push(self.parse_type_argument()?);
        }
        Some(args)
    }

    fn parse_type_argument(&mut self) -> Option<TypeUse> {
        match self.peek()? {
            b'*' => {
                self.bump();
                Some(TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Wildcard(None),
                })
            }
            b'+' | b'-' => {
                self.bump();
                let bound = self.parse_reference_type_signature()?;
                Some(TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::Wildcard(Some(Box::new(bound))),
                })
            }
            _ => self.parse_reference_type_signature(),
        }
    }
}

fn nullness_from_annotation(
    constant_pool: &[ConstantPool],
    annotation: &jclassfile::attributes::Annotation,
) -> Result<Option<Nullness>> {
    let name = annotation_class_name(constant_pool, annotation)?;
    let value = match name.as_str() {
        "org/jspecify/annotations/Nullable" => Some(Nullness::Nullable),
        "org/jspecify/annotations/NonNull" => Some(Nullness::NonNull),
        "org/jspecify/annotations/NullnessUnspecified" => Some(Nullness::Unknown),
        _ => None,
    };
    Ok(value)
}

fn annotation_class_name(
    constant_pool: &[ConstantPool],
    annotation: &jclassfile::attributes::Annotation,
) -> Result<String> {
    let descriptor =
        resolve_utf8(constant_pool, annotation.type_index()).context("resolve annotation type")?;
    let trimmed = descriptor
        .strip_prefix('L')
        .and_then(|value| value.strip_suffix(';'))
        .context("invalid annotation descriptor")?;
    Ok(trimmed.to_string())
}

fn apply_nullness(target: &mut Nullness, value: Nullness) {
    if *target == Nullness::Unknown {
        *target = value;
        return;
    }
    if *target != value {
        *target = Nullness::Unknown;
    }
}

fn is_reference_type(ty: &TypeDescriptor) -> bool {
    matches!(ty, TypeDescriptor::Object(_) | TypeDescriptor::Array(_, _))
}

fn parse_bytecode(
    code: &[u8],
    constant_pool: &[ConstantPool],
) -> Result<(Vec<Instruction>, Vec<CallSite>, Vec<String>)> {
    let mut instructions = Vec::new();
    let mut calls = Vec::new();
    let mut string_literals = Vec::new();
    let mut offset = 0usize;
    while offset < code.len() {
        let opcode = code[offset];
        let start_offset = offset as u32;
        let length = opcode_length(code, offset)?;
        if length == 0 || offset + length > code.len() {
            anyhow::bail!("invalid bytecode length at offset {}", offset);
        }
        let kind = match opcode {
            opcodes::INVOKEVIRTUAL
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKESTATIC
            | opcodes::INVOKEINTERFACE => {
                let method_index = read_u16(code, offset + 1)?;
                let method_ref = resolve_method_ref(constant_pool, method_index)
                    .context("resolve method ref")?;
                let call_kind = match opcode {
                    opcodes::INVOKEVIRTUAL => CallKind::Virtual,
                    opcodes::INVOKESPECIAL => CallKind::Special,
                    opcodes::INVOKESTATIC => CallKind::Static,
                    opcodes::INVOKEINTERFACE => CallKind::Interface,
                    _ => CallKind::Virtual,
                };
                let call = CallSite {
                    owner: method_ref.owner,
                    name: method_ref.name,
                    descriptor: method_ref.descriptor,
                    kind: call_kind,
                    offset: start_offset,
                };
                calls.push(call.clone());
                InstructionKind::Invoke(call)
            }
            opcodes::LDC => {
                let index = code.get(offset + 1).copied().context("ldc index")? as u16;
                if let Some(value) = resolve_string_literal(constant_pool, index)? {
                    string_literals.push(value.clone());
                    InstructionKind::ConstString(value)
                } else if let Some(value) = resolve_class_literal(constant_pool, index)? {
                    InstructionKind::ConstClass(value)
                } else {
                    InstructionKind::Other(opcode)
                }
            }
            opcodes::LDC_W | opcodes::LDC2_W => {
                let index = read_u16(code, offset + 1)?;
                if let Some(value) = resolve_string_literal(constant_pool, index)? {
                    string_literals.push(value.clone());
                    InstructionKind::ConstString(value)
                } else if let Some(value) = resolve_class_literal(constant_pool, index)? {
                    InstructionKind::ConstClass(value)
                } else {
                    InstructionKind::Other(opcode)
                }
            }
            opcodes::INVOKEDYNAMIC => {
                let call_site_index = read_u16(code, offset + 1)?;
                let descriptor = resolve_invoke_dynamic_descriptor(constant_pool, call_site_index)
                    .context("resolve invoke dynamic descriptor")?;
                InstructionKind::InvokeDynamic { descriptor }
            }
            _ => InstructionKind::Other(opcode),
        };

        instructions.push(Instruction {
            offset: start_offset,
            opcode,
            kind,
        });
        offset += length;
    }
    Ok((instructions, calls, string_literals))
}

/// Resolved constant pool method reference.
struct MethodRef {
    owner: String,
    name: String,
    descriptor: String,
}

fn resolve_method_ref(constant_pool: &[ConstantPool], index: u16) -> Result<MethodRef> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing method ref entry")?;
    let (class_index, name_and_type_index) = match entry {
        ConstantPool::Methodref {
            class_index,
            name_and_type_index,
        } => (*class_index, *name_and_type_index),
        ConstantPool::InterfaceMethodref {
            class_index,
            name_and_type_index,
        } => (*class_index, *name_and_type_index),
        _ => anyhow::bail!("unexpected method ref entry"),
    };
    let owner = resolve_class_name(constant_pool, class_index).context("resolve owner")?;
    let (name_index, descriptor_index) = resolve_name_and_type(constant_pool, name_and_type_index)?;
    let name = resolve_utf8(constant_pool, name_index).context("resolve method name")?;
    let descriptor =
        resolve_utf8(constant_pool, descriptor_index).context("resolve method descriptor")?;
    Ok(MethodRef {
        owner,
        name,
        descriptor,
    })
}

fn resolve_invoke_dynamic_descriptor(constant_pool: &[ConstantPool], index: u16) -> Result<String> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing invoke dynamic entry")?;
    let name_and_type_index = match entry {
        ConstantPool::InvokeDynamic {
            name_and_type_index,
            ..
        } => *name_and_type_index,
        _ => anyhow::bail!("unexpected invoke dynamic entry"),
    };
    let (_, descriptor_index) = resolve_name_and_type(constant_pool, name_and_type_index)?;
    resolve_utf8(constant_pool, descriptor_index).context("resolve invoke dynamic descriptor")
}

fn resolve_name_and_type(constant_pool: &[ConstantPool], index: u16) -> Result<(u16, u16)> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing name and type entry")?;
    match entry {
        ConstantPool::NameAndType {
            name_index,
            descriptor_index,
        } => Ok((*name_index, *descriptor_index)),
        _ => anyhow::bail!("unexpected name and type entry"),
    }
}

pub(crate) fn opcode_length(code: &[u8], offset: usize) -> Result<usize> {
    let opcode = code[offset];
    let length = match opcode {
        0x00..=0x0f => 1,
        0x10 => 2,
        0x11 => 3,
        opcodes::LDC => 2,
        opcodes::LDC_W | opcodes::LDC2_W => 3,
        0x15..=0x19 => 2,
        0x1a..=0x35 => 1,
        0x36..=0x3a => 2,
        0x3b..=0x4e => 1,
        0x4f..=0x56 => 1,
        0x57..=0x5f => 1,
        0x60..=0x83 => 1,
        0x84 => 3,
        0x85..=0x98 => 1,
        0x99..=0xa6 => 3,
        opcodes::GOTO | opcodes::JSR => 3,
        0xa9 => 2,
        0xaa => tableswitch_length(code, offset)?,
        0xab => lookupswitch_length(code, offset)?,
        0xac..=0xb1 => 1,
        0xb2..=0xb5 => 3,
        opcodes::INVOKEVIRTUAL | opcodes::INVOKESPECIAL | opcodes::INVOKESTATIC => 3,
        opcodes::INVOKEINTERFACE | opcodes::INVOKEDYNAMIC => 5,
        0xbb => 3,
        0xbc => 2,
        0xbd => 3,
        0xbe | 0xbf => 1,
        0xc0 | 0xc1 => 3,
        0xc2 | 0xc3 => 1,
        0xc4 => wide_length(code, offset)?,
        0xc5 => 4,
        0xc6 | 0xc7 => 3,
        opcodes::GOTO_W | opcodes::JSR_W => 5,
        0xca => 1,
        0xfe | 0xff => 1,
        _ => anyhow::bail!("unsupported opcode 0x{:02x}", opcode),
    };
    Ok(length)
}

fn tableswitch_length(code: &[u8], offset: usize) -> Result<usize> {
    let padding = padding(offset);
    let base = offset + 1 + padding;
    let low = read_i32(code, base + 4)?;
    let high = read_i32(code, base + 8)?;
    let count = high
        .checked_sub(low)
        .and_then(|v| v.checked_add(1))
        .context("invalid tableswitch range")?;
    if count < 0 {
        anyhow::bail!("invalid tableswitch range");
    }
    Ok(1 + padding + 12 + (count as usize) * 4)
}

fn lookupswitch_length(code: &[u8], offset: usize) -> Result<usize> {
    let padding = padding(offset);
    let base = offset + 1 + padding;
    let npairs = read_i32(code, base + 4)?;
    if npairs < 0 {
        anyhow::bail!("invalid lookupswitch pairs");
    }
    Ok(1 + padding + 8 + (npairs as usize) * 8)
}

fn wide_length(code: &[u8], offset: usize) -> Result<usize> {
    let opcode = code
        .get(offset + 1)
        .copied()
        .context("missing wide opcode")?;
    if opcode == 0x84 { Ok(6) } else { Ok(4) }
}

pub(crate) fn padding(offset: usize) -> usize {
    (4 - ((offset + 1) % 4)) % 4
}

pub(crate) fn read_u16(code: &[u8], offset: usize) -> Result<u16> {
    let slice = code
        .get(offset..offset + 2)
        .context("bytecode u16 out of bounds")?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

pub(crate) fn read_u32(code: &[u8], offset: usize) -> Result<u32> {
    let slice = code
        .get(offset..offset + 4)
        .context("bytecode u32 out of bounds")?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_i32(code: &[u8], offset: usize) -> Result<i32> {
    let value = read_u32(code, offset)?;
    Ok(i32::from_be_bytes(value.to_be_bytes()))
}

fn resolve_string_literal(constant_pool: &[ConstantPool], index: u16) -> Result<Option<String>> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing constant pool entry")?;
    match entry {
        ConstantPool::String { string_index } => {
            let value = resolve_utf8(constant_pool, *string_index)?;
            Ok(Some(value))
        }
        ConstantPool::Utf8 { value } => Ok(Some(value.clone())),
        _ => Ok(None),
    }
}

fn resolve_class_literal(constant_pool: &[ConstantPool], index: u16) -> Result<Option<String>> {
    let entry = constant_pool
        .get(index as usize)
        .context("missing constant pool entry")?;
    match entry {
        ConstantPool::Class { name_index } => Ok(Some(resolve_utf8(constant_pool, *name_index)?)),
        _ => Ok(None),
    }
}

fn parse_exception_handlers(
    table: &[jclassfile::attributes::ExceptionRecord],
    constant_pool: &[ConstantPool],
) -> Result<Vec<ExceptionHandler>> {
    let mut handlers = Vec::new();
    for entry in table {
        let catch_type = if entry.catch_type() == 0 {
            None
        } else {
            Some(
                resolve_class_name(constant_pool, entry.catch_type())
                    .context("resolve catch type")?,
            )
        };
        handlers.push(ExceptionHandler {
            start_pc: entry.start_pc() as u32,
            end_pc: entry.end_pc() as u32,
            handler_pc: entry.handler_pc() as u32,
            catch_type,
        });
    }
    Ok(handlers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::io::Read;
    use std::io::Write;
    use std::sync::OnceLock;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::ZipArchive;
    use zip::write::SimpleFileOptions;

    #[test]
    fn scan_inputs_rejects_invalid_class_file() {
        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let class_path = temp_dir.join("bad.class");
        fs::write(&class_path, b"nope").expect("write test class");

        let result = scan_inputs(&[class_path.clone()], &[], None);

        assert!(result.is_err());
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn scan_inputs_accepts_valid_jar() {
        let jar_path = jspecify_jar_path().expect("download jar");
        let result = scan_inputs(&[jar_path.clone()], &[], None).expect("scan jar");

        assert!(result.class_count > 0);
        assert_eq!(result.artifacts.len(), 1);
        let first_uri = result
            .artifacts
            .first()
            .and_then(|artifact| artifact.location.as_ref())
            .and_then(|location| location.uri.as_ref())
            .cloned()
            .expect("artifact uri");
        assert!(first_uri.ends_with("jspecify-1.0.0.jar"));
    }

    #[test]
    fn scan_inputs_accepts_valid_class_file() {
        let jar_path = jspecify_jar_path().expect("download jar");
        let class_bytes = extract_first_class(&jar_path).expect("extract class");

        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let class_path = temp_dir.join("Sample.class");
        fs::write(&class_path, class_bytes).expect("write class file");

        let result = scan_inputs(&[class_path.clone()], &[], None).expect("scan class");

        assert_eq!(result.class_count, 1);
        assert_eq!(result.artifacts.len(), 1);
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn scan_inputs_marks_directory_entries_as_targets() {
        let jar_path = jspecify_jar_path().expect("download jar");
        let class_bytes = extract_first_class(&jar_path).expect("extract class");

        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let class_path = temp_dir.join("Sample.class");
        fs::write(&class_path, class_bytes).expect("write class file");

        let result = scan_inputs(&[temp_dir.clone()], &[], None).expect("scan directory");

        assert_eq!(result.class_count, 1);
        assert_eq!(result.artifacts.len(), 1);
        let roles = result
            .artifacts
            .first()
            .and_then(|artifact| artifact.roles.as_ref())
            .expect("artifact roles");
        assert!(
            roles
                .iter()
                .any(|role| role.as_str() == Some("analysisTarget")),
            "roles: {roles:?}"
        );
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn scan_inputs_resolves_manifest_classpath() {
        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let dep_path = temp_dir.join("dep.jar");
        create_manifest_jar(&dep_path, None).expect("create dep jar");
        let jar_path = temp_dir.join("main.jar");
        create_manifest_jar(&jar_path, Some("dep.jar")).expect("create main jar");

        let result = scan_inputs(&[jar_path.clone()], &[], None);

        assert!(result.is_ok());
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn scan_inputs_errors_on_missing_manifest_classpath_entry() {
        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");

        let jar_path = temp_dir.join("main.jar");
        create_manifest_jar(&jar_path, Some("missing.jar")).expect("create main jar");

        let result = scan_inputs(&[jar_path.clone()], &[], None);

        assert!(result.is_err());
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn scan_inputs_resolves_nested_manifest_classpath() {
        let jar_path = jspecify_jar_path().expect("download jar");
        let class_bytes = extract_first_class(&jar_path).expect("extract class");

        let inner_jar =
            build_jar_bytes_with_class(Some("sibling.jar"), "Sample.class", &class_bytes)
                .expect("build inner jar");
        let sibling_jar = build_jar_bytes_with_class(None, "Sample.class", &class_bytes)
            .expect("build sibling jar");

        let temp_dir = std::env::temp_dir().join(format!(
            "inspequte-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let outer_path = temp_dir.join("outer.jar");
        create_outer_jar_with_entries(
            &outer_path,
            &[
                ("lib/inner.jar", inner_jar),
                ("lib/sibling.jar", sibling_jar),
            ],
        )
        .expect("create outer jar");

        let result = scan_inputs(&[outer_path.clone()], &[], None).expect("scan outer jar");

        assert_eq!(result.class_count, 2);
        assert_eq!(result.artifacts.len(), 3);
        fs::remove_dir_all(&temp_dir).expect("cleanup temp dir");
    }

    #[test]
    fn default_nullness_parses_marked_and_unmarked() {
        let constant_pool = vec![
            ConstantPool::Utf8 {
                value: String::new(),
            },
            ConstantPool::Utf8 {
                value: "Lorg/jspecify/annotations/NullMarked;".to_string(),
            },
            ConstantPool::Utf8 {
                value: "Lorg/jspecify/annotations/NullUnmarked;".to_string(),
            },
        ];
        let marked = jclassfile::attributes::Annotation::new(1, Vec::new());
        let unmarked = jclassfile::attributes::Annotation::new(2, Vec::new());

        let marked_attr = jclassfile::attributes::Attribute::RuntimeVisibleAnnotations {
            annotations: vec![marked],
            raw: Vec::new(),
        };
        let unmarked_attr = jclassfile::attributes::Attribute::RuntimeVisibleAnnotations {
            annotations: vec![unmarked],
            raw: Vec::new(),
        };

        let marked_default =
            parse_default_nullness(&[marked_attr.clone()], &constant_pool).expect("marked default");
        let unmarked_default =
            parse_default_nullness(&[unmarked_attr], &constant_pool).expect("unmarked default");
        let empty_default = parse_default_nullness(&[], &constant_pool).expect("empty default");

        assert_eq!(marked_default, DefaultNullness::NonNull);
        assert_eq!(unmarked_default, DefaultNullness::Unmarked);
        assert_eq!(empty_default, DefaultNullness::Inherit);
    }

    #[test]
    fn default_nullness_applies_to_reference_types() {
        let constant_pool = vec![ConstantPool::Utf8 {
            value: String::new(),
        }];
        let nullness = parse_method_nullness(
            &constant_pool,
            &[],
            "(Ljava/lang/String;)Ljava/lang/String;",
            DefaultNullness::NonNull,
        )
        .expect("method nullness");

        assert_eq!(nullness.parameter_nullness.len(), 1);
        assert_eq!(nullness.parameter_nullness[0], Nullness::NonNull);
        assert_eq!(nullness.return_nullness, Nullness::NonNull);
    }

    #[test]
    fn nullunmarked_overrides_class_default() {
        let constant_pool = vec![
            ConstantPool::Utf8 {
                value: String::new(),
            },
            ConstantPool::Utf8 {
                value: "Lorg/jspecify/annotations/NullUnmarked;".to_string(),
            },
        ];
        let unmarked = jclassfile::attributes::Annotation::new(1, Vec::new());
        let unmarked_attr = jclassfile::attributes::Attribute::RuntimeVisibleAnnotations {
            annotations: vec![unmarked],
            raw: Vec::new(),
        };
        let nullness = parse_method_nullness(
            &constant_pool,
            &[unmarked_attr],
            "(Ljava/lang/String;)Ljava/lang/String;",
            DefaultNullness::NonNull,
        )
        .expect("method nullness");

        assert_eq!(nullness.parameter_nullness[0], Nullness::Unknown);
        assert_eq!(nullness.return_nullness, Nullness::Unknown);
    }

    fn extract_first_class(jar_path: &Path) -> Result<Vec<u8>> {
        let file =
            fs::File::open(jar_path).with_context(|| format!("open {}", jar_path.display()))?;
        let mut archive =
            ZipArchive::new(file).with_context(|| format!("read {}", jar_path.display()))?;
        for index in 0..archive.len() {
            let mut entry = archive
                .by_index(index)
                .with_context(|| format!("read {}:{}", jar_path.display(), index))?;
            if entry.is_dir()
                || !entry.name().ends_with(".class")
                || entry.name().ends_with("module-info.class")
            {
                continue;
            }
            let mut data = Vec::new();
            entry.read_to_end(&mut data).context("read class bytes")?;
            return Ok(data);
        }

        anyhow::bail!("no class entry found in {}", jar_path.display());
    }

    fn create_manifest_jar(path: &Path, class_path: Option<&str>) -> Result<()> {
        let file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut writer = zip::ZipWriter::new(file);
        let mut manifest = String::from("Manifest-Version: 1.0\n");
        if let Some(class_path) = class_path {
            manifest.push_str(&format!("Class-Path: {class_path}\n"));
        }
        manifest.push('\n');
        writer
            .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .context("start manifest entry")?;
        writer
            .write_all(manifest.as_bytes())
            .context("write manifest")?;
        writer.finish().context("finish jar")?;
        Ok(())
    }

    fn build_jar_bytes_with_class(
        class_path: Option<&str>,
        entry_name: &str,
        class_bytes: &[u8],
    ) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        let cursor = Cursor::new(&mut buffer);
        let mut writer = zip::ZipWriter::new(cursor);
        let mut manifest = String::from("Manifest-Version: 1.0\n");
        if let Some(class_path) = class_path {
            manifest.push_str(&format!("Class-Path: {class_path}\n"));
        }
        manifest.push('\n');
        writer
            .start_file("META-INF/MANIFEST.MF", SimpleFileOptions::default())
            .context("start manifest entry")?;
        writer
            .write_all(manifest.as_bytes())
            .context("write manifest")?;
        writer
            .start_file(entry_name, SimpleFileOptions::default())
            .context("start class entry")?;
        writer.write_all(class_bytes).context("write class bytes")?;
        writer.finish().context("finish jar")?;
        Ok(buffer)
    }

    fn create_outer_jar_with_entries(path: &Path, entries: &[(&str, Vec<u8>)]) -> Result<()> {
        let file = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
        let mut writer = zip::ZipWriter::new(file);
        for (name, data) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .with_context(|| format!("start entry {}", name))?;
            writer
                .write_all(data)
                .with_context(|| format!("write entry {}", name))?;
        }
        writer.finish().context("finish jar")?;
        Ok(())
    }

    fn jspecify_jar_path() -> Result<PathBuf> {
        static JAR_PATH: OnceLock<PathBuf> = OnceLock::new();
        if let Some(path) = JAR_PATH.get() {
            return Ok(path.clone());
        }
        let jar_path = download_jspecify_jar()?;
        let _ = JAR_PATH.set(jar_path.clone());
        Ok(jar_path)
    }

    fn download_jspecify_jar() -> Result<PathBuf> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-fixtures");
        fs::create_dir_all(&dir).context("create fixture directory")?;
        let jar_path = dir.join("jspecify-1.0.0.jar");
        if jar_path.exists() {
            return Ok(jar_path);
        }

        let url =
            "https://repo.maven.apache.org/maven2/org/jspecify/jspecify/1.0.0/jspecify-1.0.0.jar";
        let mut response = ureq::get(url).call().context("download jspecify jar")?;
        if response.status().as_u16() >= 400 {
            anyhow::bail!(
                "failed to download jspecify jar: HTTP {}",
                response.status()
            );
        }

        let mut reader = response.body_mut().as_reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .context("read jspecify jar")?;
        fs::write(&jar_path, bytes).context("write jspecify jar")?;

        Ok(jar_path)
    }
}
