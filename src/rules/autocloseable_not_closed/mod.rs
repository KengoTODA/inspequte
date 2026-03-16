use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::dataflow::opcode_semantics::{
    ApplyOutcome, SemanticsCoverage, SemanticsDebugConfig, SemanticsHooks, ValueDomain,
    apply_semantics,
};
use crate::dataflow::stack_machine::StackMachine;
use crate::dataflow::worklist::{
    BlockEndStep, InstructionStep, WorklistSemantics, WorklistState, analyze_method,
};
use crate::descriptor::{ReturnKind, method_descriptor_summary, method_return_class_name};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, CallSite, Class, EdgeKind, Instruction, InstructionKind, Method};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

const MAX_TRACKED_STACK_DEPTH: usize = 32;

/// Rule that detects locally created AutoCloseable instances without guaranteed close().
#[derive(Default)]
pub(crate) struct UnmanagedAutocloseableRule;

crate::register_rule!(UnmanagedAutocloseableRule);

impl Rule for UnmanagedAutocloseableRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "AUTOCLOSEABLE_NOT_CLOSED",
            name: "AutoCloseable not closed",
            description: "Locally created AutoCloseable instances should be closed on every exit path",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        let class_map = context
            .all_classes()
            .map(|class| (class.name.clone(), class))
            .collect::<BTreeMap<_, _>>();

        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }

            let class_results =
                context.with_span("rule.class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    let guidance = if class
                        .source_file
                        .as_ref()
                        .is_some_and(|f| f.ends_with(".kt"))
                    {
                        ".use {}"
                    } else {
                        "try-with-resources"
                    };

                    for method in &class.methods {
                        // Abstract/native methods have no Code attribute and are excluded
                        // by the scanner, so this guard is defensive only.
                        if method.bytecode.is_empty() || method.cfg.blocks.is_empty() {
                            continue;
                        }

                        for creation_offset in analyze_closeable_lifecycle(method, &class_map)? {
                            let cls_name = &class.name;
                            let met_name = &method.name;
                            let met_descriptor = &method.descriptor;
                            let message = result_message(format!(
                                "AutoCloseable created in {cls_name}.{met_name}{met_descriptor} \
                                may not be closed on all paths; use {guidance} or call close() in a finally block.",
                            ));
                            let line = method.line_for_offset(creation_offset);
                            let location = method_location_with_line(
                                cls_name,
                                met_name,
                                met_descriptor,
                                context.class_artifact_uri(class).as_deref(),
                                line,
                            );
                            class_results.push(
                                SarifResult::builder()
                                    .message(message)
                                    .locations(vec![location])
                                    .build(),
                            );
                        }
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }

        Ok(results)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Value {
    Unknown,
    Symbol(u32),
}

/// Value-domain adapter for default opcode semantics.
struct CloseableValueDomain;

impl ValueDomain<Value> for CloseableValueDomain {
    fn unknown_value(&self) -> Value {
        Value::Unknown
    }

    fn scalar_value(&self) -> Value {
        Value::Unknown
    }
}

/// Symbolic execution state for local AutoCloseable ownership tracking.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ExecutionState {
    block_start: u32,
    instruction_index: usize,
    machine: StackMachine<Value>,
    active_closeables: BTreeSet<u32>,
    branch_filter: Option<BranchFilter>,
}

impl WorklistState for ExecutionState {
    fn block_start(&self) -> u32 {
        self.block_start
    }

    fn instruction_index(&self) -> usize {
        self.instruction_index
    }

    fn set_position(&mut self, block_start: u32, instruction_index: usize) {
        self.block_start = block_start;
        self.instruction_index = instruction_index;
    }
}

/// Dataflow callbacks for local AutoCloseable lifecycle analysis.
struct CloseableLifecycleSemantics<'a> {
    entry_block: u32,
    class_map: &'a BTreeMap<String, &'a Class>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum BranchFilter {
    TakeBranch,
    TakeFallThrough,
}

impl WorklistSemantics for CloseableLifecycleSemantics<'_> {
    type State = ExecutionState;
    type Finding = u32;

    fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
        vec![ExecutionState {
            block_start: self.entry_block,
            instruction_index: 0,
            machine: StackMachine::with_config(
                Value::Unknown,
                crate::dataflow::stack_machine::StackMachineConfig {
                    max_stack_depth: Some(MAX_TRACKED_STACK_DEPTH),
                    max_locals: None,
                    max_symbolic_identities: None,
                },
            ),
            active_closeables: BTreeSet::new(),
            branch_filter: None,
        }]
    }

    fn transfer_instruction(
        &self,
        method: &Method,
        instruction: &Instruction,
        state: &mut Self::State,
    ) -> Result<InstructionStep<Self::Finding>> {
        state.branch_filter = None;
        match instruction.opcode {
            opcodes::AASTORE => {
                escape_top_symbol(state);
                state.machine.pop_n(3);
            }
            opcodes::ARETURN => {
                escape_value(state.machine.pop(), state);
            }
            opcodes::ATHROW => {
                escape_value(state.machine.pop(), state);
            }
            opcodes::PUTSTATIC => {
                escape_top_symbol(state);
                state.machine.pop_n(1);
            }
            opcodes::PUTFIELD => {
                escape_top_symbol(state);
                state.machine.pop_n(2);
            }
            opcodes::IFNULL | opcodes::IFNONNULL => {
                let value = state.machine.pop();
                if let Value::Symbol(symbol) = value
                    && state.active_closeables.contains(&symbol)
                {
                    state.branch_filter = Some(if instruction.opcode == opcodes::IFNULL {
                        BranchFilter::TakeFallThrough
                    } else {
                        BranchFilter::TakeBranch
                    });
                }
            }
            _ => match &instruction.kind {
                InstructionKind::Invoke(call) => handle_invoke(call, state, self.class_map)?,
                InstructionKind::InvokeDynamic { descriptor, .. } => {
                    handle_invoke_dynamic(descriptor, state)?
                }
                _ => apply_stack_effect(method, instruction, state),
            },
        }

        Ok(InstructionStep::continue_path())
    }

    fn on_block_end(
        &self,
        method: &Method,
        state: &Self::State,
        successors: &[u32],
    ) -> Result<BlockEndStep<Self::State, Self::Finding>> {
        if successors.is_empty() {
            let mut step = BlockEndStep::terminal();
            for creation_offset in &state.active_closeables {
                step = step.with_finding(*creation_offset);
            }
            return Ok(step);
        }

        let filtered_successors = match state.branch_filter {
            Some(filter) => branch_successors(method, state.block_start, filter),
            None => Vec::new(),
        };
        if !filtered_successors.is_empty() {
            return Ok(follow_successors_without_branch_filter(
                state,
                &filtered_successors,
            ));
        }

        Ok(follow_successors_without_branch_filter(state, successors))
    }
}

fn analyze_closeable_lifecycle(
    method: &Method,
    class_map: &BTreeMap<String, &Class>,
) -> Result<Vec<u32>> {
    let entry_block = method
        .cfg
        .blocks
        .iter()
        .map(|block| block.start_offset)
        .min()
        .unwrap_or(0);
    let semantics = CloseableLifecycleSemantics {
        entry_block,
        class_map,
    };
    let findings = analyze_method(method, &semantics)?;
    Ok(findings
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn apply_stack_effect(method: &Method, instruction: &Instruction, state: &mut ExecutionState) {
    let domain = CloseableValueDomain;
    let mut hook = CloseableSemanticsHook {
        allocation_offset: instruction.offset,
    };
    let mut coverage = SemanticsCoverage::default();
    let _ = apply_semantics(
        &mut state.machine,
        method,
        instruction.offset as usize,
        instruction.opcode,
        &domain,
        &mut hook,
        &mut coverage,
        SemanticsDebugConfig::default(),
    );
}

/// Rule-specific hook that gives each new reference allocation a symbolic ID.
struct CloseableSemanticsHook {
    allocation_offset: u32,
}

impl SemanticsHooks<Value> for CloseableSemanticsHook {
    fn pre_apply(
        &mut self,
        machine: &mut StackMachine<Value>,
        _method: &Method,
        _offset: usize,
        opcode: u8,
    ) -> ApplyOutcome {
        if opcode == opcodes::NEW {
            machine.push(Value::Symbol(self.allocation_offset));
            return ApplyOutcome::Applied;
        }
        ApplyOutcome::NotHandled
    }
}

fn handle_invoke(
    call: &CallSite,
    state: &mut ExecutionState,
    class_map: &BTreeMap<String, &Class>,
) -> Result<()> {
    let summary = method_descriptor_summary(&call.descriptor)?;
    let mut args = Vec::with_capacity(summary.param_count);
    for _ in 0..summary.param_count {
        args.push(state.machine.pop());
    }

    let receiver = if call.kind == CallKind::Static {
        None
    } else {
        Some(state.machine.pop())
    };

    if call.name == "<init>" {
        let is_excluded = is_excluded_noop_type(&call.owner);
        if let Some(Value::Symbol(symbol)) = receiver
            && !is_excluded
            && is_autocloseable_constructor(call, class_map)
        {
            // Tracked AutoCloseable: wrapper delegation escapes inner args.
            state.active_closeables.insert(symbol);
            for value in args {
                escape_value(value, state);
            }
        } else {
            // Clear the NEW symbol for non-tracked constructors.
            if let Some(Value::Symbol(symbol)) = receiver {
                state.machine.rewrite_values(|value| {
                    if *value == Value::Symbol(symbol) {
                        *value = Value::Unknown;
                    }
                });
            }
            // Escape args unless the outer is an excluded no-op type
            // (its close() won't close inner resources).
            if !is_excluded {
                for value in args {
                    escape_value(value, state);
                }
            }
        }
        return Ok(());
    }

    // Check if this is a close() call on a tracked symbol.
    let close_receiver = if is_close_call(call) {
        receiver.and_then(|value| match value {
            Value::Symbol(symbol) if state.active_closeables.contains(&symbol) => Some(symbol),
            _ => None,
        })
    } else {
        None
    };

    // Escape all arguments.
    for value in args {
        escape_value(value, state);
    }

    if let Some(symbol) = close_receiver {
        state.active_closeables.remove(&symbol);
    }

    // Check if the return type is AutoCloseable — if so, track it.
    if summary.return_kind == ReturnKind::Reference {
        if let Ok(Some(return_class)) = method_return_class_name(&call.descriptor) {
            if !is_excluded_noop_type(&return_class)
                && is_autocloseable_type(&return_class, class_map)
            {
                let symbol = call.offset;
                state.active_closeables.insert(symbol);
                state.machine.push(Value::Symbol(symbol));
                return Ok(());
            }
        }
        state.machine.push(Value::Unknown);
        return Ok(());
    }

    if summary.return_kind != ReturnKind::Void {
        state.machine.push(Value::Unknown);
    }

    Ok(())
}

fn handle_invoke_dynamic(descriptor: &str, state: &mut ExecutionState) -> Result<()> {
    let summary = method_descriptor_summary(descriptor)?;
    let mut args = Vec::with_capacity(summary.param_count);
    for _ in 0..summary.param_count {
        args.push(state.machine.pop());
    }
    for value in args {
        escape_value(value, state);
    }
    if summary.return_kind != ReturnKind::Void {
        state.machine.push(Value::Unknown);
    }
    Ok(())
}

fn branch_successors(method: &Method, block_start: u32, filter: BranchFilter) -> Vec<u32> {
    let mut selected = method
        .cfg
        .edges
        .iter()
        .filter(|edge| edge.from == block_start)
        .filter_map(|edge| match (filter, edge.kind) {
            (BranchFilter::TakeBranch, EdgeKind::Branch) => Some(edge.to),
            (BranchFilter::TakeFallThrough, EdgeKind::FallThrough) => Some(edge.to),
            _ => None,
        })
        .collect::<Vec<_>>();
    selected.sort();
    selected.dedup();
    selected
}

fn follow_successors_without_branch_filter(
    state: &ExecutionState,
    successors: &[u32],
) -> BlockEndStep<ExecutionState, u32> {
    let mut next = state.clone();
    next.branch_filter = None;
    BlockEndStep::follow_all_successors(&next, successors)
}

fn escape_top_symbol(state: &mut ExecutionState) {
    if let Some(value) = state.machine.stack_values().last().copied() {
        escape_value(value, state);
    }
}

fn escape_value(value: Value, state: &mut ExecutionState) {
    if let Value::Symbol(symbol) = value {
        state.active_closeables.remove(&symbol);
    }
}

fn is_close_call(call: &CallSite) -> bool {
    call.name == "close" && call.descriptor == "()V"
}

fn is_autocloseable_constructor(call: &CallSite, class_map: &BTreeMap<String, &Class>) -> bool {
    call.name == "<init>" && is_autocloseable_type(&call.owner, class_map)
}

fn is_autocloseable_type(name: &str, class_map: &BTreeMap<String, &Class>) -> bool {
    if is_known_autocloseable_name(name) {
        return true;
    }

    let mut queue = VecDeque::from([name.to_string()]);
    let mut seen = BTreeSet::new();
    while let Some(next) = queue.pop_front() {
        if !seen.insert(next.clone()) {
            continue;
        }
        if is_known_autocloseable_name(&next) {
            return true;
        }
        let Some(class) = class_map.get(&next) else {
            continue;
        };
        if let Some(super_name) = &class.super_name {
            queue.push_back(super_name.clone());
        }
        for interface in &class.interfaces {
            queue.push_back(interface.clone());
        }
    }

    false
}

/// All public AutoCloseable types from Java 21 JDK (excluding types in the
/// no-op exclusion list). Generated via jrt-fs scan of JDK 21.0.2.
fn is_known_autocloseable_name(name: &str) -> bool {
    matches!(
        name,
        "java/lang/AutoCloseable"
            | "java/io/Closeable"
            // java.beans
            | "java/beans/XMLDecoder"
            | "java/beans/XMLEncoder"
            // InputStream family
            | "java/io/InputStream"
            | "java/io/FileInputStream"
            | "java/io/BufferedInputStream"
            | "java/io/DataInputStream"
            | "java/io/FilterInputStream"
            | "java/io/LineNumberInputStream"
            | "java/io/ObjectInput"
            | "java/io/ObjectInputStream"
            | "java/io/PipedInputStream"
            | "java/io/PushbackInputStream"
            | "java/io/SequenceInputStream"
            | "java/security/DigestInputStream"
            | "java/util/zip/CheckedInputStream"
            | "java/util/zip/DeflaterInputStream"
            | "java/util/zip/GZIPInputStream"
            | "java/util/zip/InflaterInputStream"
            | "java/util/zip/ZipInputStream"
            | "java/util/jar/JarInputStream"
            | "javax/crypto/CipherInputStream"
            | "javax/swing/ProgressMonitorInputStream"
            // OutputStream family
            | "java/io/OutputStream"
            | "java/io/FileOutputStream"
            | "java/io/BufferedOutputStream"
            | "java/io/DataOutputStream"
            | "java/io/FilterOutputStream"
            | "java/io/ObjectOutput"
            | "java/io/ObjectOutputStream"
            | "java/io/PipedOutputStream"
            | "java/io/PrintStream"
            | "java/security/DigestOutputStream"
            | "java/util/zip/CheckedOutputStream"
            | "java/util/zip/DeflaterOutputStream"
            | "java/util/zip/GZIPOutputStream"
            | "java/util/zip/InflaterOutputStream"
            | "java/util/zip/ZipOutputStream"
            | "java/util/jar/JarOutputStream"
            | "javax/crypto/CipherOutputStream"
            // Reader family
            | "java/io/Reader"
            | "java/io/FileReader"
            | "java/io/BufferedReader"
            | "java/io/InputStreamReader"
            | "java/io/FilterReader"
            | "java/io/LineNumberReader"
            | "java/io/PipedReader"
            | "java/io/PushbackReader"
            // Writer family
            | "java/io/Writer"
            | "java/io/FileWriter"
            | "java/io/BufferedWriter"
            | "java/io/OutputStreamWriter"
            | "java/io/FilterWriter"
            | "java/io/PipedWriter"
            | "java/io/PrintWriter"
            // RandomAccessFile
            | "java/io/RandomAccessFile"
            // JDBC
            | "java/sql/Connection"
            | "java/sql/Statement"
            | "java/sql/PreparedStatement"
            | "java/sql/CallableStatement"
            | "java/sql/ResultSet"
            // java.lang.foreign
            | "java/lang/foreign/Arena"
            // java.lang.module
            | "java/lang/module/ModuleReader"
            // Network
            | "java/net/DatagramSocket"
            | "java/net/MulticastSocket"
            | "java/net/ServerSocket"
            | "java/net/Socket"
            | "java/net/URLClassLoader"
            // NIO channels
            | "java/nio/channels/AsynchronousByteChannel"
            | "java/nio/channels/AsynchronousChannel"
            | "java/nio/channels/AsynchronousFileChannel"
            | "java/nio/channels/AsynchronousServerSocketChannel"
            | "java/nio/channels/AsynchronousSocketChannel"
            | "java/nio/channels/ByteChannel"
            | "java/nio/channels/Channel"
            | "java/nio/channels/DatagramChannel"
            | "java/nio/channels/FileChannel"
            | "java/nio/channels/FileLock"
            | "java/nio/channels/GatheringByteChannel"
            | "java/nio/channels/InterruptibleChannel"
            | "java/nio/channels/MulticastChannel"
            | "java/nio/channels/NetworkChannel"
            | "java/nio/channels/ReadableByteChannel"
            | "java/nio/channels/ScatteringByteChannel"
            | "java/nio/channels/SeekableByteChannel"
            | "java/nio/channels/SelectableChannel"
            | "java/nio/channels/Selector"
            | "java/nio/channels/ServerSocketChannel"
            | "java/nio/channels/SocketChannel"
            | "java/nio/channels/WritableByteChannel"
            | "java/nio/channels/spi/AbstractInterruptibleChannel"
            | "java/nio/channels/spi/AbstractSelectableChannel"
            | "java/nio/channels/spi/AbstractSelector"
            // java.nio.file
            | "java/nio/file/DirectoryStream"
            | "java/nio/file/FileSystem"
            | "java/nio/file/SecureDirectoryStream"
            | "java/nio/file/WatchService"
            // java.rmi
            | "java/rmi/server/LogStream"
            // java.util
            | "java/util/Formatter"
            | "java/util/Scanner"
            // java.util.concurrent
            | "java/util/concurrent/AbstractExecutorService"
            | "java/util/concurrent/ExecutorService"
            | "java/util/concurrent/ForkJoinPool"
            | "java/util/concurrent/ScheduledExecutorService"
            | "java/util/concurrent/ScheduledThreadPoolExecutor"
            | "java/util/concurrent/StructuredTaskScope"
            | "java/util/concurrent/SubmissionPublisher"
            | "java/util/concurrent/ThreadPoolExecutor"
            // java.util.jar / zip
            | "java/util/jar/JarFile"
            | "java/util/zip/ZipFile"
            // javax.imageio.stream
            | "javax/imageio/stream/FileCacheImageInputStream"
            | "javax/imageio/stream/FileCacheImageOutputStream"
            | "javax/imageio/stream/FileImageInputStream"
            | "javax/imageio/stream/FileImageOutputStream"
            | "javax/imageio/stream/ImageInputStream"
            | "javax/imageio/stream/ImageOutputStream"
            // javax.management
            | "javax/management/loading/MLet"
            | "javax/management/loading/PrivateMLet"
            | "javax/management/remote/JMXConnector"
            | "javax/management/remote/rmi/RMIConnection"
            | "javax/management/remote/rmi/RMIConnectionImpl"
            | "javax/management/remote/rmi/RMIConnectionImpl_Stub"
            | "javax/management/remote/rmi/RMIConnector"
            | "javax/management/remote/rmi/RMIJRMPServerImpl"
            | "javax/management/remote/rmi/RMIServerImpl"
            // javax.net.ssl
            | "javax/net/ssl/SSLServerSocket"
            | "javax/net/ssl/SSLSocket"
            // javax.sound.midi
            | "javax/sound/midi/MidiDevice"
            | "javax/sound/midi/MidiDeviceReceiver"
            | "javax/sound/midi/MidiDeviceTransmitter"
            | "javax/sound/midi/Receiver"
            | "javax/sound/midi/Sequencer"
            | "javax/sound/midi/Synthesizer"
            | "javax/sound/midi/Transmitter"
            // javax.sound.sampled
            | "javax/sound/sampled/AudioInputStream"
            | "javax/sound/sampled/Clip"
            | "javax/sound/sampled/DataLine"
            | "javax/sound/sampled/Line"
            | "javax/sound/sampled/Mixer"
            | "javax/sound/sampled/Port"
            | "javax/sound/sampled/SourceDataLine"
            | "javax/sound/sampled/TargetDataLine"
            // javax.tools
            | "javax/tools/ForwardingJavaFileManager"
            | "javax/tools/JavaFileManager"
            | "javax/tools/StandardJavaFileManager"
    )
}

fn is_excluded_noop_type(name: &str) -> bool {
    matches!(
        name,
        "java/io/ByteArrayOutputStream"
            | "java/io/ByteArrayInputStream"
            | "java/io/StringBufferInputStream"
            | "java/io/CharArrayWriter"
            | "java/io/CharArrayReader"
            | "java/io/StringWriter"
            | "java/io/StringReader"
            // ImageIO impls (close() just sets boolean flag or resets in-memory cache)
            | "javax/imageio/stream/ImageInputStreamImpl"
            | "javax/imageio/stream/ImageOutputStreamImpl"
            | "javax/imageio/stream/MemoryCacheImageInputStream"
            | "javax/imageio/stream/MemoryCacheImageOutputStream"
            // Stream types (close() is no-op for collection-backed streams)
            | "java/util/stream/BaseStream"
            | "java/util/stream/Stream"
            | "java/util/stream/IntStream"
            | "java/util/stream/LongStream"
            | "java/util/stream/DoubleStream"
    )
}

#[cfg(test)]
mod tests {
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn analyze_java_sources(sources: Vec<SourceFile>) -> Vec<String> {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let output = harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis");
        output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("AUTOCLOSEABLE_NOT_CLOSED"))
            .filter_map(|result| result.message.text.clone())
            .collect()
    }

    #[test]
    fn reports_unclosed_autocloseable() {
        let sources = vec![SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassA {
    public void methodX() throws Exception {
        InputStream varOne = new FileInputStream("f.txt");
        varOne.read();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("may not be closed on all paths"));
        assert!(messages[0].contains("use try-with-resources"));
    }

    #[test]
    fn reports_unclosed_autocloseable_kotlin_message() {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        let sources = vec![SourceFile {
            path: "com/example/ClassA.kt".to_string(),
            contents: r#"
package com.example

import java.io.FileInputStream

class ClassA {
    fun methodX() {
        val varOne = FileInputStream("f.txt")
        varOne.read()
    }
}
"#
            .to_string(),
        }];
        let output = harness
            .compile_and_analyze(Language::Kotlin, &sources, &[])
            .expect("run harness analysis");
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("AUTOCLOSEABLE_NOT_CLOSED"))
            .filter_map(|result| result.message.text.clone())
            .collect();
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("may not be closed on all paths"));
        assert!(
            messages[0].contains("use .use {}"),
            "Kotlin message should suggest .use {{}}, got: {messages:?}",
        );
    }

    #[test]
    fn does_not_report_try_with_resources() {
        let sources = vec![SourceFile {
            path: "com/example/ClassB.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassB {
    public void methodX() throws Exception {
        try (InputStream varOne = new FileInputStream("f.txt")) {
            varOne.read();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_finally_close() {
        let sources = vec![SourceFile {
            path: "com/example/ClassC.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassC {
    public void methodX() throws Exception {
        InputStream varOne = new FileInputStream("f.txt");
        try {
            varOne.read();
        } finally {
            varOne.close();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_escape_via_field() {
        let sources = vec![SourceFile {
            path: "com/example/ClassD.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassD {
    private InputStream fieldOne;

    public void methodX() throws Exception {
        fieldOne = new FileInputStream("f.txt");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_escape_via_return() {
        let sources = vec![SourceFile {
            path: "com/example/ClassE.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassE {
    public InputStream methodX() throws Exception {
        return new FileInputStream("f.txt");
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_escape_via_argument() {
        let sources = vec![SourceFile {
            path: "com/example/ClassF.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassF {
    public void methodX() throws Exception {
        InputStream varOne = new FileInputStream("f.txt");
        helperMethod(varOne);
    }

    private void helperMethod(InputStream varOne) {}
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_escape_via_array() {
        let sources = vec![SourceFile {
            path: "com/example/ClassG.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassG {
    public void methodX() throws Exception {
        Object[] varOne = new Object[1];
        InputStream varTwo = new FileInputStream("f.txt");
        varOne[0] = varTwo;
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_wrapper_pattern() {
        let sources = vec![SourceFile {
            path: "com/example/ClassH.java".to_string(),
            contents: r#"
package com.example;

import java.io.BufferedReader;
import java.io.FileReader;

public class ClassH {
    public void methodX() throws Exception {
        BufferedReader varOne = new BufferedReader(new FileReader("f.txt"));
        try {
            varOne.readLine();
        } finally {
            varOne.close();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn does_not_report_excluded_noop_types() {
        let sources = vec![SourceFile {
            path: "com/example/ClassI.java".to_string(),
            contents: r#"
package com.example;

import java.io.ByteArrayOutputStream;
import java.io.StringWriter;
import java.io.StringReader;

public class ClassI {
    public byte[] methodX() {
        ByteArrayOutputStream varOne = new ByteArrayOutputStream();
        varOne.write(42);
        return varOne.toByteArray();
    }

    public String methodY() {
        StringWriter varOne = new StringWriter();
        varOne.write("hello");
        return varOne.toString();
    }

    public int methodZ() throws Exception {
        StringReader varOne = new StringReader("hello");
        return varOne.read();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn reports_early_return_before_close() {
        let sources = vec![SourceFile {
            path: "com/example/ClassJ.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;

public class ClassJ {
    public void methodX(boolean varOne) throws Exception {
        InputStream varTwo = new FileInputStream("f.txt");
        if (varOne) {
            return;
        }
        varTwo.close();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn reports_partial_close_for_multiple_resources() {
        let sources = vec![SourceFile {
            path: "com/example/ClassK.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

public class ClassK {
    public void methodX() throws Exception {
        InputStream varOne = new FileInputStream("in.txt");
        OutputStream varTwo = new FileOutputStream("out.txt");
        try {
            varOne.read();
            varTwo.write(42);
        } finally {
            varOne.close();
        }
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(
            messages.len(),
            1,
            "expected exactly one finding for varTwo: {messages:?}"
        );
    }

    #[test]
    fn reports_factory_method_returning_autocloseable() {
        let sources = vec![SourceFile {
            path: "com/example/ClassL.java".to_string(),
            contents: r#"
package com.example;

import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;

public class ClassL {
    public void methodX() throws Exception {
        InputStream varOne = Files.newInputStream(Path.of("f.txt"));
        varOne.read();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("may not be closed on all paths"));
    }

    #[test]
    fn does_not_report_non_autocloseable() {
        let sources = vec![SourceFile {
            path: "com/example/ClassM.java".to_string(),
            contents: r#"
package com.example;

public class ClassM {
    public void methodX() {
        Object varOne = new Object();
        varOne.toString();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(messages.is_empty(), "did not expect finding: {messages:?}");
    }

    #[test]
    fn reports_app_defined_autocloseable_subclass() {
        let sources = vec![SourceFile {
            path: "com/example/ClassN.java".to_string(),
            contents: r#"
package com.example;

public class ClassN {
    public void methodX() {
        ClassResource varOne = new ClassResource();
        varOne.doWork();
    }

    static class ClassResource implements AutoCloseable {
        void doWork() {}

        @Override
        public void close() {}
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("may not be closed on all paths"));
    }

    #[test]
    fn reports_inner_resource_wrapped_by_excluded_noop_type() {
        let sources = vec![SourceFile {
            path: "com/example/ClassO.java".to_string(),
            contents: r#"
package com.example;

import java.io.FileInputStream;
import java.io.InputStream;
import javax.imageio.stream.MemoryCacheImageInputStream;

public class ClassO {
    public void methodX() throws Exception {
        InputStream varOne = new FileInputStream("f.txt");
        MemoryCacheImageInputStream varTwo = new MemoryCacheImageInputStream(varOne);
        varTwo.readByte();
        varTwo.close();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert_eq!(
            messages.len(),
            1,
            "inner resource should be reported when wrapped by excluded no-op type: {messages:?}"
        );
    }

    #[test]
    fn does_not_report_null_guard_before_close() {
        let sources = vec![SourceFile {
            path: "com/example/ClassQ.java".to_string(),
            contents: r#"
package com.example;

import java.io.InputStream;

public class ClassQ {
    static InputStream getStream() { return null; }

    public void methodX() throws Exception {
        InputStream varOne = getStream();
        if (varOne == null) return;
        varOne.close();
    }
}
"#
            .to_string(),
        }];

        let messages = analyze_java_sources(sources);
        assert!(
            messages.is_empty(),
            "null-guarded close should not be reported: {messages:?}"
        );
    }

}
