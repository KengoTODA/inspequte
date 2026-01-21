use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use tempfile::TempDir;

use crate::classpath::resolve_classpath;
use crate::engine::{Engine, EngineOutput, build_context};
use crate::scan::scan_inputs;

/// Supported JVM source languages for the harness.
#[allow(dead_code)]
pub(crate) enum Language {
    Java,
    Kotlin,
}

/// Source file definition for compilation.
pub(crate) struct SourceFile {
    pub(crate) path: String,
    pub(crate) contents: String,
}

/// Compiled output directories from the harness.
pub(crate) struct CompileOutput {
    #[allow(dead_code)]
    temp_dir: TempDir,
    classes_dir: PathBuf,
}

impl CompileOutput {
    pub(crate) fn classes_dir(&self) -> &Path {
        &self.classes_dir
    }

    #[allow(dead_code)]
    pub(crate) fn temp_dir(&self) -> &TempDir {
        &self.temp_dir
    }
}

/// Test harness that compiles JVM sources and runs analysis.
pub(crate) struct JvmTestHarness {
    javac: PathBuf,
    kotlinc: Option<PathBuf>,
}

impl JvmTestHarness {
    pub(crate) fn new() -> Result<Self> {
        let javac = javac_path()?;
        let kotlinc = kotlinc_path();
        Ok(Self { javac, kotlinc })
    }

    pub(crate) fn compile(
        &self,
        language: Language,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> Result<CompileOutput> {
        let temp_dir = tempfile::tempdir().context("create temp dir")?;
        let src_dir = temp_dir.path().join("src");
        let classes_dir = temp_dir.path().join("classes");
        fs::create_dir_all(&src_dir).context("create src dir")?;
        fs::create_dir_all(&classes_dir).context("create classes dir")?;

        let mut source_paths = Vec::new();
        for source in sources {
            let path = src_dir.join(&source.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).context("create source parent dir")?;
            }
            fs::write(&path, source.contents.as_bytes()).context("write source file")?;
            source_paths.push(path);
        }

        match language {
            Language::Java => {
                let mut command = Command::new(&self.javac);
                command.arg("-d").arg(&classes_dir);
                if let Some(cp) = classpath_arg(classpath) {
                    command.arg("-classpath").arg(cp);
                }
                command.args(&source_paths);
                run_command(command, "javac")?;
            }
            Language::Kotlin => {
                let kotlinc = self
                    .kotlinc
                    .as_ref()
                    .context("kotlinc not available; set KOTLIN_HOME or PATH")?;
                let mut command = Command::new(kotlinc);
                command.arg("-d").arg(&classes_dir);
                if let Some(cp) = classpath_arg(classpath) {
                    command.arg("-classpath").arg(cp);
                }
                command.args(&source_paths);
                run_command(command, "kotlinc")?;
            }
        }

        Ok(CompileOutput {
            temp_dir,
            classes_dir,
        })
    }

    pub(crate) fn analyze(
        &self,
        classes_dir: &Path,
        classpath: &[PathBuf],
    ) -> Result<EngineOutput> {
        let scan = scan_inputs(classes_dir, classpath, None).context("scan classes")?;
        let classpath_index = resolve_classpath(&scan.classes).context("resolve classpath")?;
        let context = build_context(scan.classes, classpath_index, &scan.artifacts);
        let engine = Engine::new();
        engine.analyze(context).context("run analysis")
    }

    pub(crate) fn compile_and_analyze(
        &self,
        language: Language,
        sources: &[SourceFile],
        classpath: &[PathBuf],
    ) -> Result<EngineOutput> {
        let output = self.compile(language, sources, classpath)?;
        self.analyze(output.classes_dir(), classpath)
    }
}

fn javac_path() -> Result<PathBuf> {
    let java_home = std::env::var("JAVA_HOME").context("JAVA_HOME not set")?;
    let mut path = PathBuf::from(java_home);
    path.push("bin");
    path.push("javac");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    if !path.exists() {
        anyhow::bail!("javac not found at {}", path.display());
    }
    Ok(path)
}

fn kotlinc_path() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("KOTLIN_HOME") {
        let mut path = PathBuf::from(home);
        path.push("bin");
        path.push("kotlinc");
        if cfg!(windows) {
            path.set_extension("bat");
        }
        if path.exists() {
            return Some(path);
        }
    }
    let path = PathBuf::from("kotlinc");
    if path.exists() {
        return Some(path);
    }
    None
}

fn classpath_arg(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let sep = if cfg!(windows) { ";" } else { ":" };
    let joined = paths
        .iter()
        .map(|path| path.to_string_lossy())
        .collect::<Vec<_>>()
        .join(sep);
    Some(joined)
}

fn run_command(mut command: Command, label: &str) -> Result<()> {
    let output = command.output().with_context(|| format!("run {label}"))?;
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{label} failed: stdout={stdout} stderr={stderr}");
    }
    Ok(())
}
