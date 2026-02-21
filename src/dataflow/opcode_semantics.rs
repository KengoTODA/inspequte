use crate::dataflow::stack_machine::StackMachine;
use crate::ir::Method;
use crate::opcodes;
use opentelemetry::KeyValue;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use tracing::info;

/// Rule-supplied value constructors used by shared opcode semantics.
pub(crate) trait ValueDomain<V> {
    fn unknown_value(&self) -> V;
    fn scalar_value(&self) -> V;
}

/// Result of attempting shared opcode execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum ApplyOutcome {
    Applied,
    NotHandled,
}

/// Optional debug controls for opcode semantics instrumentation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct SemanticsDebugConfig {
    pub(crate) enabled: bool,
    pub(crate) rule_id: &'static str,
}

/// Coverage counters for shared opcode semantics execution.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SemanticsCoverage {
    pub(crate) applied_by_hook: usize,
    pub(crate) applied_by_default: usize,
    pub(crate) fallback_not_handled: usize,
    overridden_opcodes: BTreeMap<u8, usize>,
    fallback_opcodes: BTreeMap<u8, usize>,
}

impl SemanticsCoverage {
    fn record_hook_override(&mut self, opcode: u8) {
        self.applied_by_hook += 1;
        *self.overridden_opcodes.entry(opcode).or_insert(0) += 1;
    }

    fn record_default_apply(&mut self) {
        self.applied_by_default += 1;
    }

    fn record_fallback(&mut self, opcode: u8) {
        self.fallback_not_handled += 1;
        *self.fallback_opcodes.entry(opcode).or_insert(0) += 1;
    }

    /// Returns how often a specific opcode was overridden by a hook.
    #[cfg(test)]
    pub(crate) fn hook_override_count(&self, opcode: u8) -> usize {
        self.overridden_opcodes.get(&opcode).copied().unwrap_or(0)
    }

    /// Returns how often a specific opcode was left to fallback handling.
    pub(crate) fn fallback_count(&self, opcode: u8) -> usize {
        self.fallback_opcodes.get(&opcode).copied().unwrap_or(0)
    }

    /// Merges another coverage snapshot into this one.
    pub(crate) fn merge_from(&mut self, other: &SemanticsCoverage) {
        self.applied_by_hook += other.applied_by_hook;
        self.applied_by_default += other.applied_by_default;
        self.fallback_not_handled += other.fallback_not_handled;
        for (opcode, count) in &other.overridden_opcodes {
            *self.overridden_opcodes.entry(*opcode).or_insert(0) += count;
        }
        for (opcode, count) in &other.fallback_opcodes {
            *self.fallback_opcodes.entry(*opcode).or_insert(0) += count;
        }
    }
}

/// Rule hook points around default opcode semantics.
pub(crate) trait SemanticsHooks<V> {
    fn pre_apply(
        &mut self,
        _machine: &mut StackMachine<V>,
        _method: &Method,
        _offset: usize,
        _opcode: u8,
    ) -> ApplyOutcome {
        ApplyOutcome::NotHandled
    }

    fn post_apply(
        &mut self,
        _machine: &mut StackMachine<V>,
        _method: &Method,
        _offset: usize,
        _opcode: u8,
        _outcome: ApplyOutcome,
    ) {
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
struct NoHooks;

#[cfg(test)]
impl<V> SemanticsHooks<V> for NoHooks {}

/// Applies table-driven semantics with optional rule hooks and instrumentation.
pub(crate) fn apply_semantics<V, D, H>(
    machine: &mut StackMachine<V>,
    method: &Method,
    offset: usize,
    opcode: u8,
    domain: &D,
    hooks: &mut H,
    coverage: &mut SemanticsCoverage,
    debug_config: SemanticsDebugConfig,
) -> ApplyOutcome
where
    V: Clone,
    D: ValueDomain<V>,
    H: SemanticsHooks<V>,
{
    if hooks.pre_apply(machine, method, offset, opcode) == ApplyOutcome::Applied {
        coverage.record_hook_override(opcode);
        if debug_config.enabled {
            info!(
                "opcode_semantics debug: rule={} offset={} opcode=0x{:02x} event=hook_override",
                debug_config.rule_id, offset, opcode
            );
        }
        hooks.post_apply(machine, method, offset, opcode, ApplyOutcome::Applied);
        return ApplyOutcome::Applied;
    }

    let outcome = if let Some(effect) = decode(opcode) {
        apply_effect(machine, method, offset, domain, effect);
        coverage.record_default_apply();
        ApplyOutcome::Applied
    } else {
        coverage.record_fallback(opcode);
        if debug_config.enabled {
            info!(
                "opcode_semantics debug: rule={} offset={} opcode=0x{:02x} event=fallback",
                debug_config.rule_id, offset, opcode
            );
        }
        ApplyOutcome::NotHandled
    };

    hooks.post_apply(machine, method, offset, opcode, outcome);
    outcome
}

/// Applies table-driven default semantics when the opcode is recognized.
#[cfg(test)]
pub(crate) fn apply_default_semantics<V, D>(
    machine: &mut StackMachine<V>,
    method: &Method,
    offset: usize,
    opcode: u8,
    domain: &D,
) -> ApplyOutcome
where
    V: Clone,
    D: ValueDomain<V>,
{
    let mut hooks = NoHooks;
    let mut coverage = SemanticsCoverage::default();
    apply_semantics(
        machine,
        method,
        offset,
        opcode,
        domain,
        &mut hooks,
        &mut coverage,
        SemanticsDebugConfig::default(),
    )
}

/// Returns whether opcode semantics debug logging is enabled.
pub(crate) fn opcode_semantics_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("INSPEQUTE_DEBUG_OPCODE_SEMANTICS")
            .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
    })
}

/// Emits one summary event for opcode semantics fallback/debug counters.
pub(crate) fn emit_opcode_semantics_summary_event(rule_id: &str, coverage: &SemanticsCoverage) {
    let invoke_fallbacks = coverage.fallback_count(opcodes::INVOKEVIRTUAL)
        + coverage.fallback_count(opcodes::INVOKESPECIAL)
        + coverage.fallback_count(opcodes::INVOKESTATIC)
        + coverage.fallback_count(opcodes::INVOKEINTERFACE)
        + coverage.fallback_count(opcodes::INVOKEDYNAMIC);
    let attributes = [
        KeyValue::new("inspequte.rule_id", rule_id.to_string()),
        KeyValue::new("inspequte.debug_summary", "opcode_semantics"),
        KeyValue::new(
            "inspequte.fallback_count",
            coverage.fallback_not_handled as i64,
        ),
        KeyValue::new(
            "inspequte.default_apply_count",
            coverage.applied_by_default as i64,
        ),
        KeyValue::new(
            "inspequte.hook_apply_count",
            coverage.applied_by_hook as i64,
        ),
        KeyValue::new("inspequte.invoke_fallback_count", invoke_fallbacks as i64),
    ];
    crate::telemetry::add_current_span_event("inspequte.debug.summary", &attributes);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Effect {
    Noop,
    PushUnknown,
    PushScalar,
    LoadLocal(LocalSlot),
    StoreLocal(LocalSlot),
    Pop(usize),
    Dup,
    Dup2,
    Swap,
    PopAndPush { pop_count: usize, push: PushKind },
    MultiANewArray,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum PushKind {
    Unknown,
    Scalar,
}

fn apply_effect<V, D>(
    machine: &mut StackMachine<V>,
    method: &Method,
    offset: usize,
    domain: &D,
    effect: Effect,
) where
    V: Clone,
    D: ValueDomain<V>,
{
    match effect {
        Effect::Noop => {}
        Effect::PushUnknown => machine.push(domain.unknown_value()),
        Effect::PushScalar => machine.push(domain.scalar_value()),
        Effect::LoadLocal(slot) => {
            machine.push(machine.load_local(local_index(method, offset, slot)));
        }
        Effect::StoreLocal(slot) => {
            let value = machine.pop();
            machine.store_local(local_index(method, offset, slot), value);
        }
        Effect::Pop(count) => machine.pop_n(count),
        Effect::Dup => {
            if let Some(value) = machine.peek().cloned() {
                machine.push(value);
            }
        }
        Effect::Dup2 => {
            let len = machine.stack_len();
            if len >= 2 {
                if let (Some(left), Some(right)) = (
                    machine.stack_values().get(len - 2).cloned(),
                    machine.stack_values().get(len - 1).cloned(),
                ) {
                    machine.push(left);
                    machine.push(right);
                }
            } else if let Some(value) = machine.peek().cloned() {
                machine.push(value.clone());
                machine.push(value);
            }
        }
        Effect::Swap => {
            let right = machine.pop();
            let left = machine.pop();
            machine.push(right);
            machine.push(left);
        }
        Effect::PopAndPush { pop_count, push } => {
            machine.pop_n(pop_count);
            match push {
                PushKind::Unknown => machine.push(domain.unknown_value()),
                PushKind::Scalar => machine.push(domain.scalar_value()),
            }
        }
        Effect::MultiANewArray => {
            let dims = method.bytecode.get(offset + 3).copied().unwrap_or(1) as usize;
            machine.pop_n(dims);
            machine.push(domain.unknown_value());
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum LocalSlot {
    OperandU8,
    Fixed(usize),
}

fn decode(opcode: u8) -> Option<Effect> {
    let effect = match opcode {
        opcodes::NOP => Effect::Noop,
        opcodes::ACONST_NULL => Effect::PushUnknown,
        opcodes::ICONST_M1
        | opcodes::ICONST_0
        | opcodes::ICONST_1
        | opcodes::ICONST_2
        | opcodes::ICONST_3
        | opcodes::ICONST_4
        | opcodes::ICONST_5
        | opcodes::BIPUSH
        | opcodes::SIPUSH
        | opcodes::NEW
        | opcodes::LDC
        | opcodes::LDC_W
        | opcodes::LDC2_W => Effect::PushScalar,
        // Primitive loads.
        opcodes::ILOAD
        | opcodes::ILOAD_0
        | opcodes::ILOAD_1
        | opcodes::ILOAD_2
        | opcodes::ILOAD_3
        | 0x16..=0x18
        | 0x1e..=0x29 => Effect::PushScalar,
        opcodes::ALOAD => Effect::LoadLocal(LocalSlot::OperandU8),
        opcodes::ALOAD_0 => Effect::LoadLocal(LocalSlot::Fixed(0)),
        opcodes::ALOAD_1 => Effect::LoadLocal(LocalSlot::Fixed(1)),
        opcodes::ALOAD_2 => Effect::LoadLocal(LocalSlot::Fixed(2)),
        opcodes::ALOAD_3 => Effect::LoadLocal(LocalSlot::Fixed(3)),
        // Primitive stores.
        0x36 | 0x38 | 0x3b..=0x3e | 0x43..=0x46 => Effect::Pop(1),
        0x37 | 0x39 | 0x3f..=0x42 | 0x47..=0x4a => Effect::Pop(2),
        opcodes::ASTORE => Effect::StoreLocal(LocalSlot::OperandU8),
        opcodes::ASTORE_0 => Effect::StoreLocal(LocalSlot::Fixed(0)),
        opcodes::ASTORE_1 => Effect::StoreLocal(LocalSlot::Fixed(1)),
        opcodes::ASTORE_2 => Effect::StoreLocal(LocalSlot::Fixed(2)),
        opcodes::ASTORE_3 => Effect::StoreLocal(LocalSlot::Fixed(3)),
        // Primitive array loads.
        0x2e..=0x31 | 0x33..=0x35 => Effect::PopAndPush {
            pop_count: 2,
            push: PushKind::Scalar,
        },
        // Primitive array stores.
        0x4f | 0x51 | 0x52 | 0x54..=0x56 => Effect::Pop(3),
        0x50 => Effect::Pop(4),
        opcodes::AASTORE => Effect::Pop(3),
        opcodes::AALOAD => Effect::PopAndPush {
            pop_count: 2,
            push: PushKind::Unknown,
        },
        opcodes::POP => Effect::Pop(1),
        opcodes::POP2 => Effect::Pop(2),
        opcodes::DUP => Effect::Dup,
        0x5a | 0x5b => Effect::Dup,
        0x5c..=0x5e => Effect::Dup2,
        0x5f => Effect::Swap,
        // Primitive arithmetic and compare.
        0x60..=0x73 | 0x78..=0x83 | 0x94..=0x98 => Effect::PopAndPush {
            pop_count: 2,
            push: PushKind::Scalar,
        },
        0x74..=0x77 | 0x85..=0x93 => Effect::PopAndPush {
            pop_count: 1,
            push: PushKind::Scalar,
        },
        // iinc has no stack effect.
        0x84 => Effect::Noop,
        opcodes::IFEQ
        | opcodes::IFNE
        | opcodes::IFLT
        | opcodes::IFGE
        | opcodes::IFGT
        | opcodes::IFLE
        | opcodes::IFNULL
        | opcodes::IFNONNULL
        | opcodes::TABLESWITCH
        | opcodes::LOOKUPSWITCH => Effect::Pop(1),
        opcodes::IF_ICMPEQ
        | opcodes::IF_ICMPNE
        | opcodes::IF_ICMPLT
        | opcodes::IF_ICMPGE
        | opcodes::IF_ICMPGT
        | opcodes::IF_ICMPLE
        | opcodes::IF_ACMPEQ
        | opcodes::IF_ACMPNE => Effect::Pop(2),
        opcodes::GOTO | opcodes::GOTO_W => Effect::Noop,
        opcodes::JSR | opcodes::JSR_W => Effect::PushScalar,
        // Field access.
        0xb2 => Effect::PushScalar,
        0xb3 => Effect::Pop(1),
        0xb4 => Effect::PopAndPush {
            pop_count: 1,
            push: PushKind::Scalar,
        },
        0xb5 => Effect::Pop(2),
        // Array/type/monitor.
        opcodes::NEWARRAY | opcodes::ANEWARRAY => Effect::PopAndPush {
            pop_count: 1,
            push: PushKind::Unknown,
        },
        opcodes::ARRAYLENGTH | 0xc1 => Effect::PopAndPush {
            pop_count: 1,
            push: PushKind::Scalar,
        },
        0xc0 => Effect::PopAndPush {
            pop_count: 1,
            push: PushKind::Unknown,
        },
        0xc2 | 0xc3 => Effect::Pop(1),
        opcodes::MULTIANEWARRAY => Effect::MultiANewArray,
        _ => return None,
    };
    Some(effect)
}

fn local_index(method: &Method, offset: usize, slot: LocalSlot) -> usize {
    match slot {
        LocalSlot::OperandU8 => method.bytecode.get(offset + 1).copied().unwrap_or(0) as usize,
        LocalSlot::Fixed(index) => index,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyOutcome, SemanticsCoverage, SemanticsDebugConfig, SemanticsHooks, ValueDomain,
        apply_default_semantics, apply_semantics,
    };
    use crate::dataflow::stack_machine::StackMachine;
    use crate::ir::{
        ControlFlowGraph, LineNumber, LocalVariableType, Method, MethodAccess, MethodNullness,
        Nullness,
    };
    use crate::opcodes;

    #[derive(Clone, Copy)]
    struct TestDomain;

    impl ValueDomain<i32> for TestDomain {
        fn unknown_value(&self) -> i32 {
            -1
        }

        fn scalar_value(&self) -> i32 {
            1
        }
    }

    fn empty_method(bytecode: Vec<u8>) -> Method {
        Method {
            name: "MethodX".to_string(),
            descriptor: "()V".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: false,
                is_static: true,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::Unknown,
                parameter_nullness: Vec::new(),
            },
            type_use: None,
            bytecode,
            line_numbers: Vec::<LineNumber>::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::<LocalVariableType>::new(),
        }
    }

    #[test]
    fn applies_load_store_and_stack_ops() {
        let method = empty_method(vec![opcodes::ASTORE, 2, opcodes::ALOAD, 2]);
        let mut machine = StackMachine::new(-1);
        machine.push(7);
        let domain = TestDomain;

        assert_eq!(
            apply_default_semantics(&mut machine, &method, 0, opcodes::ASTORE, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(
            apply_default_semantics(&mut machine, &method, 2, opcodes::ALOAD, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(machine.pop(), 7);
    }

    #[test]
    fn applies_arithmetic_field_array_and_control_ops() {
        let method = empty_method(vec![
            0x60, // iadd
            0xb4, // getfield
            0x2e, // iaload
            opcodes::IFEQ,
            0x00,
            0x00,
        ]);
        let domain = TestDomain;

        let mut arith_machine = StackMachine::new(-1);
        arith_machine.push(10);
        arith_machine.push(20);
        assert_eq!(
            apply_default_semantics(&mut arith_machine, &method, 0, 0x60, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(arith_machine.pop(), 1);

        let mut field_machine = StackMachine::new(-1);
        field_machine.push(7);
        assert_eq!(
            apply_default_semantics(&mut field_machine, &method, 1, 0xb4, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(field_machine.pop(), 1);

        let mut array_machine = StackMachine::new(-1);
        array_machine.push(2);
        array_machine.push(3);
        assert_eq!(
            apply_default_semantics(&mut array_machine, &method, 2, 0x2e, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(array_machine.pop(), 1);

        let mut control_machine = StackMachine::new(-1);
        control_machine.push(1);
        assert_eq!(
            apply_default_semantics(&mut control_machine, &method, 3, opcodes::IFEQ, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(control_machine.stack_len(), 0);
    }

    #[test]
    fn applies_multianewarray_using_dimension_operand() {
        let method = empty_method(vec![opcodes::MULTIANEWARRAY, 0, 1, 2]);
        let mut machine = StackMachine::new(-1);
        let domain = TestDomain;
        machine.push(7);
        machine.push(8);
        assert_eq!(
            apply_default_semantics(&mut machine, &method, 0, opcodes::MULTIANEWARRAY, &domain),
            ApplyOutcome::Applied
        );
        assert_eq!(machine.stack_len(), 1);
        assert_eq!(machine.pop(), -1);
    }

    /// Test hook that overrides one opcode and records post outcomes.
    struct TestHook {
        post_outcomes: Vec<ApplyOutcome>,
    }

    impl SemanticsHooks<i32> for TestHook {
        fn pre_apply(
            &mut self,
            machine: &mut StackMachine<i32>,
            _method: &Method,
            _offset: usize,
            opcode: u8,
        ) -> ApplyOutcome {
            if opcode == opcodes::AALOAD {
                machine.push(42);
                return ApplyOutcome::Applied;
            }
            ApplyOutcome::NotHandled
        }

        fn post_apply(
            &mut self,
            _machine: &mut StackMachine<i32>,
            _method: &Method,
            _offset: usize,
            _opcode: u8,
            outcome: ApplyOutcome,
        ) {
            self.post_outcomes.push(outcome);
        }
    }

    #[test]
    fn hook_precedence_and_fallback_are_counted() {
        let method = empty_method(vec![opcodes::AALOAD, opcodes::INVOKEVIRTUAL]);
        let mut machine = StackMachine::new(-1);
        let domain = TestDomain;
        let mut coverage = SemanticsCoverage::default();
        let mut hook = TestHook {
            post_outcomes: Vec::new(),
        };

        assert_eq!(
            apply_semantics(
                &mut machine,
                &method,
                0,
                opcodes::AALOAD,
                &domain,
                &mut hook,
                &mut coverage,
                SemanticsDebugConfig {
                    enabled: false,
                    rule_id: "TEST",
                },
            ),
            ApplyOutcome::Applied
        );
        assert_eq!(
            apply_semantics(
                &mut machine,
                &method,
                1,
                opcodes::INVOKEVIRTUAL,
                &domain,
                &mut hook,
                &mut coverage,
                SemanticsDebugConfig {
                    enabled: false,
                    rule_id: "TEST",
                },
            ),
            ApplyOutcome::NotHandled
        );
        assert_eq!(coverage.applied_by_hook, 1);
        assert_eq!(coverage.applied_by_default, 0);
        assert_eq!(coverage.fallback_not_handled, 1);
        assert_eq!(coverage.hook_override_count(opcodes::AALOAD), 1);
        assert_eq!(coverage.fallback_count(opcodes::INVOKEVIRTUAL), 1);
        assert_eq!(
            hook.post_outcomes,
            vec![ApplyOutcome::Applied, ApplyOutcome::NotHandled]
        );
    }
}
