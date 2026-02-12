use crate::dataflow::stack_machine::StackMachine;
use crate::ir::Method;
use crate::opcodes;

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

/// Applies table-driven default semantics when the opcode is recognized.
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
    let Some(effect) = decode(opcode) else {
        return ApplyOutcome::NotHandled;
    };

    match effect {
        Effect::PushUnknown => {
            machine.push(domain.unknown_value());
        }
        Effect::PushScalar => {
            machine.push(domain.scalar_value());
        }
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
    }

    ApplyOutcome::Applied
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum Effect {
    PushUnknown,
    PushScalar,
    LoadLocal(LocalSlot),
    StoreLocal(LocalSlot),
    Pop(usize),
    Dup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum LocalSlot {
    OperandU8,
    Fixed(usize),
}

fn decode(opcode: u8) -> Option<Effect> {
    let effect = match opcode {
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
        | opcodes::ILOAD
        | opcodes::ILOAD_0
        | opcodes::ILOAD_1
        | opcodes::ILOAD_2
        | opcodes::ILOAD_3
        | opcodes::NEW
        | opcodes::LDC
        | opcodes::LDC_W
        | opcodes::LDC2_W => Effect::PushScalar,
        opcodes::ALOAD => Effect::LoadLocal(LocalSlot::OperandU8),
        opcodes::ALOAD_0 => Effect::LoadLocal(LocalSlot::Fixed(0)),
        opcodes::ALOAD_1 => Effect::LoadLocal(LocalSlot::Fixed(1)),
        opcodes::ALOAD_2 => Effect::LoadLocal(LocalSlot::Fixed(2)),
        opcodes::ALOAD_3 => Effect::LoadLocal(LocalSlot::Fixed(3)),
        opcodes::ASTORE => Effect::StoreLocal(LocalSlot::OperandU8),
        opcodes::ASTORE_0 => Effect::StoreLocal(LocalSlot::Fixed(0)),
        opcodes::ASTORE_1 => Effect::StoreLocal(LocalSlot::Fixed(1)),
        opcodes::ASTORE_2 => Effect::StoreLocal(LocalSlot::Fixed(2)),
        opcodes::ASTORE_3 => Effect::StoreLocal(LocalSlot::Fixed(3)),
        opcodes::POP => Effect::Pop(1),
        opcodes::POP2 => Effect::Pop(2),
        opcodes::DUP => Effect::Dup,
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
        | opcodes::IF_ICMPLE => Effect::Pop(2),
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
    use super::{ApplyOutcome, ValueDomain, apply_default_semantics};
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
    fn reports_not_handled_for_custom_opcode() {
        let method = empty_method(vec![opcodes::AALOAD]);
        let mut machine = StackMachine::new(-1);
        let domain = TestDomain;

        assert_eq!(
            apply_default_semantics(&mut machine, &method, 0, opcodes::AALOAD, &domain),
            ApplyOutcome::NotHandled
        );
    }
}
