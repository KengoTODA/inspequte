#![allow(dead_code)]

/// Intermediate representation for parsed JVM classes and methods.
#[derive(Clone, Debug)]
pub(crate) struct Class {
    pub(crate) name: String,
    pub(crate) source_file: Option<String>,
    pub(crate) super_name: Option<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) type_parameters: Vec<TypeParameterUse>,
    pub(crate) referenced_classes: Vec<String>,
    pub(crate) fields: Vec<Field>,
    pub(crate) methods: Vec<Method>,
    pub(crate) annotation_defaults: Vec<AnnotationDefaultValue>,
    pub(crate) artifact_index: i64,
    pub(crate) is_record: bool,
}

/// Numeric default value from an annotation method's AnnotationDefault attribute.
#[derive(Clone, Debug)]
pub(crate) struct AnnotationDefaultValue {
    pub(crate) method_name: String,
    pub(crate) method_descriptor: String,
    pub(crate) value: AnnotationDefaultNumeric,
}

/// Numeric type of an annotation default value.
#[derive(Clone, Debug)]
pub(crate) enum AnnotationDefaultNumeric {
    Int(i64),
    Float(f64),
}

/// Field definition for a class.
#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) descriptor: String,
    pub(crate) signature: Option<String>,
    pub(crate) type_use: Option<TypeUse>,
    pub(crate) access: FieldAccess,
}

/// Field access flags used for rule filtering.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FieldAccess {
    pub(crate) is_static: bool,
    pub(crate) is_private: bool,
    pub(crate) is_final: bool,
}

/// Intermediate representation for a method and its bytecode.
#[derive(Clone, Debug)]
pub(crate) struct Method {
    pub(crate) name: String,
    pub(crate) descriptor: String,
    pub(crate) signature: Option<String>,
    pub(crate) access: MethodAccess,
    pub(crate) nullness: MethodNullness,
    pub(crate) type_use: Option<MethodTypeUse>,
    pub(crate) bytecode: Vec<u8>,
    pub(crate) line_numbers: Vec<LineNumber>,
    pub(crate) cfg: ControlFlowGraph,
    pub(crate) calls: Vec<CallSite>,
    pub(crate) string_literals: Vec<String>,
    pub(crate) exception_handlers: Vec<ExceptionHandler>,
    pub(crate) local_variable_types: Vec<LocalVariableType>,
}

/// Local variable type metadata from the LocalVariableTypeTable attribute.
#[derive(Clone, Debug)]
pub(crate) struct LocalVariableType {
    pub(crate) name: String,
    pub(crate) signature: String,
    pub(crate) type_use: Option<TypeUse>,
    pub(crate) index: u16,
    pub(crate) start_pc: u32,
    pub(crate) length: u32,
}

/// Method access flags used for rule filtering.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MethodAccess {
    pub(crate) is_public: bool,
    pub(crate) is_static: bool,
    pub(crate) is_abstract: bool,
    pub(crate) is_synthetic: bool,
    pub(crate) is_bridge: bool,
}

/// Exception handler metadata from the Code attribute.
#[derive(Clone, Debug)]
pub(crate) struct ExceptionHandler {
    pub(crate) start_pc: u32,
    pub(crate) end_pc: u32,
    pub(crate) handler_pc: u32,
    pub(crate) catch_type: Option<String>,
}

/// Line number mapping entry from bytecode offsets to source lines.
#[derive(Clone, Debug)]
pub(crate) struct LineNumber {
    pub(crate) start_pc: u32,
    pub(crate) line: u32,
}

/// Basic block graph for method bytecode.
#[derive(Clone, Debug)]
pub(crate) struct ControlFlowGraph {
    pub(crate) blocks: Vec<BasicBlock>,
    pub(crate) edges: Vec<FlowEdge>,
}

/// Basic block covering a range of bytecode offsets.
#[derive(Clone, Debug)]
pub(crate) struct BasicBlock {
    pub(crate) start_offset: u32,
    pub(crate) end_offset: u32,
    pub(crate) instructions: Vec<Instruction>,
}

/// Edge between basic blocks.
#[derive(Clone, Debug)]
pub(crate) struct FlowEdge {
    pub(crate) from: u32,
    pub(crate) to: u32,
    pub(crate) kind: EdgeKind,
}

/// Edge classification used for CFG inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum EdgeKind {
    FallThrough,
    Branch,
    Exception,
}

/// Bytecode instruction captured for analysis.
#[derive(Clone, Debug)]
pub(crate) struct Instruction {
    pub(crate) offset: u32,
    pub(crate) opcode: u8,
    pub(crate) kind: InstructionKind,
}

/// Instruction kinds needed for call graph construction.
#[derive(Clone, Debug)]
pub(crate) enum InstructionKind {
    Invoke(CallSite),
    InvokeDynamic {
        descriptor: String,
        impl_method: Option<String>,
    },
    ConstString(String),
    ConstClass(String),
    /// Integer or long constant loaded via bipush, sipush, or ldc/ldc2_w.
    ConstInt(i64),
    /// Float or double constant loaded via ldc/ldc2_w.
    ConstFloat(f64),
    Other(u8),
}

/// Call site extracted from bytecode.
#[derive(Clone, Debug)]
pub(crate) struct CallSite {
    pub(crate) owner: String,
    pub(crate) name: String,
    pub(crate) descriptor: String,
    pub(crate) kind: CallKind,
    pub(crate) offset: u32,
}

/// Call opcode classification used by CHA.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) enum CallKind {
    Virtual,
    Interface,
    Special,
    Static,
}

/// Nullness classification used by JSpecify checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Nullness {
    Unknown,
    NonNull,
    Nullable,
}

/// Nullness annotations for a method signature.
#[derive(Clone, Debug)]
pub(crate) struct MethodNullness {
    pub(crate) return_nullness: Nullness,
    pub(crate) parameter_nullness: Vec<Nullness>,
}

/// Method type-use signature derived from generic metadata.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MethodTypeUse {
    pub(crate) type_parameters: Vec<TypeParameterUse>,
    pub(crate) parameters: Vec<TypeUse>,
    pub(crate) return_type: Option<TypeUse>,
}

/// Type parameter metadata with optional bounds.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TypeParameterUse {
    pub(crate) name: String,
    pub(crate) class_bound: Option<TypeUse>,
    pub(crate) interface_bounds: Vec<TypeUse>,
}

/// Type-use signature with nullness annotation metadata.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TypeUse {
    pub(crate) nullness: Nullness,
    pub(crate) kind: TypeUseKind,
}

/// Kind of type-use signature entry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TypeUseKind {
    Base(char),
    Array(Box<TypeUse>),
    TypeVar(String),
    Class(ClassTypeUse),
    Wildcard(Option<Box<TypeUse>>),
    Void,
}

/// Class type metadata with generic arguments and inner class segments.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ClassTypeUse {
    pub(crate) name: String,
    pub(crate) type_arguments: Vec<TypeUse>,
    pub(crate) inner: Option<Box<TypeUse>>,
}

impl MethodNullness {
    pub(crate) fn unknown(param_count: usize) -> Self {
        Self {
            return_nullness: Nullness::Unknown,
            parameter_nullness: vec![Nullness::Unknown; param_count],
        }
    }
}

impl Method {
    pub(crate) fn line_for_offset(&self, offset: u32) -> Option<u32> {
        let mut candidate = None;
        for entry in &self.line_numbers {
            if entry.start_pc <= offset {
                candidate = Some(entry.line);
            } else {
                break;
            }
        }
        candidate
    }
}
