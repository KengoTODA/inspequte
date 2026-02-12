use std::collections::{BTreeMap, BTreeSet};

/// Configuration for stack/local simulation budgets.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct StackMachineConfig {
    /// Maximum tracked operand stack depth.
    pub(crate) max_stack_depth: Option<usize>,
    /// Maximum tracked local variable index range.
    pub(crate) max_locals: Option<usize>,
    /// Maximum number of distinct symbolic identities kept alive.
    pub(crate) max_symbolic_identities: Option<usize>,
}

/// Generic abstract machine for JVM-like stack and local state.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct StackMachine<V> {
    stack: Vec<V>,
    locals: BTreeMap<usize, V>,
    default_value: V,
    config: StackMachineConfig,
}

impl<V> StackMachine<V>
where
    V: Clone,
{
    /// Creates a machine with default configuration.
    pub(crate) fn new(default_value: V) -> Self {
        Self::with_config(default_value, StackMachineConfig::default())
    }

    /// Creates a machine with explicit configuration.
    pub(crate) fn with_config(default_value: V, config: StackMachineConfig) -> Self {
        Self {
            stack: Vec::new(),
            locals: BTreeMap::new(),
            default_value,
            config,
        }
    }

    /// Returns stack length.
    pub(crate) fn stack_len(&self) -> usize {
        self.stack.len()
    }

    /// Returns all stack values.
    pub(crate) fn stack_values(&self) -> &[V] {
        &self.stack
    }

    /// Pushes a value onto the stack, applying depth cap if configured.
    pub(crate) fn push(&mut self, value: V) {
        if let Some(max_depth) = self.config.max_stack_depth
            && self.stack.len() >= max_depth
        {
            self.stack.remove(0);
        }
        self.stack.push(value);
    }

    /// Returns the top stack value if available.
    pub(crate) fn peek(&self) -> Option<&V> {
        self.stack.last()
    }

    /// Pops one value from stack or returns default.
    pub(crate) fn pop(&mut self) -> V {
        self.stack
            .pop()
            .unwrap_or_else(|| self.default_value.clone())
    }

    /// Pops multiple values and discards them.
    pub(crate) fn pop_n(&mut self, count: usize) {
        for _ in 0..count {
            self.pop();
        }
    }

    /// Loads a local value or returns default if missing/out of budget.
    pub(crate) fn load_local(&self, index: usize) -> V {
        if self.is_local_out_of_budget(index) {
            return self.default_value.clone();
        }
        self.locals
            .get(&index)
            .cloned()
            .unwrap_or_else(|| self.default_value.clone())
    }

    /// Stores a local value when within budget.
    pub(crate) fn store_local(&mut self, index: usize, value: V) {
        if self.is_local_out_of_budget(index) {
            return;
        }
        self.locals.insert(index, value);
    }

    /// Keeps local bindings that match the predicate.
    pub(crate) fn retain_locals<F>(&mut self, mut predicate: F)
    where
        F: FnMut(usize, &V) -> bool,
    {
        self.locals.retain(|index, value| predicate(*index, value));
    }

    /// Canonicalizes symbolic IDs to deterministic compact IDs.
    pub(crate) fn canonicalize_symbolic_ids_u32<FExtract, FAssign>(
        &mut self,
        mut extract: FExtract,
        mut assign: FAssign,
        extra_ids: impl IntoIterator<Item = u32>,
    ) -> BTreeMap<u32, u32>
    where
        FExtract: FnMut(&V) -> Option<u32>,
        FAssign: FnMut(&mut V, u32),
    {
        let mut mapping = BTreeMap::new();
        let mut next_id = 0u32;

        for value in &self.stack {
            if let Some(id) = extract(value) {
                mapping.entry(id).or_insert_with(|| {
                    let assigned = next_id;
                    next_id += 1;
                    assigned
                });
            }
        }
        for value in self.locals.values() {
            if let Some(id) = extract(value) {
                mapping.entry(id).or_insert_with(|| {
                    let assigned = next_id;
                    next_id += 1;
                    assigned
                });
            }
        }
        for id in extra_ids {
            mapping.entry(id).or_insert_with(|| {
                let assigned = next_id;
                next_id += 1;
                assigned
            });
        }

        if mapping.is_empty() {
            return mapping;
        }

        for value in &mut self.stack {
            if let Some(id) = extract(value)
                && let Some(mapped) = mapping.get(&id)
            {
                assign(value, *mapped);
            }
        }
        for value in self.locals.values_mut() {
            if let Some(id) = extract(value)
                && let Some(mapped) = mapping.get(&id)
            {
                assign(value, *mapped);
            }
        }

        mapping
    }

    /// Applies configured symbolic-identity cap and returns retained IDs.
    pub(crate) fn enforce_symbolic_identity_cap_u32<FExtract, FSetUnknown>(
        &mut self,
        mut extract: FExtract,
        mut set_unknown: FSetUnknown,
    ) -> Option<BTreeSet<u32>>
    where
        FExtract: FnMut(&V) -> Option<u32>,
        FSetUnknown: FnMut(&mut V),
    {
        let max = self.config.max_symbolic_identities?;

        let mut live_ids = BTreeSet::new();
        for value in self.stack.iter().chain(self.locals.values()) {
            if let Some(id) = extract(value) {
                live_ids.insert(id);
            }
        }
        let tracked_ids: BTreeSet<u32> = live_ids.iter().rev().take(max).copied().collect();

        for value in &mut self.stack {
            if let Some(id) = extract(value)
                && !tracked_ids.contains(&id)
            {
                set_unknown(value);
            }
        }
        for value in self.locals.values_mut() {
            if let Some(id) = extract(value)
                && !tracked_ids.contains(&id)
            {
                set_unknown(value);
            }
        }

        Some(tracked_ids)
    }

    fn is_local_out_of_budget(&self, index: usize) -> bool {
        self.config
            .max_locals
            .is_some_and(|max_locals| index >= max_locals)
    }
}

#[cfg(test)]
mod tests {
    use super::{StackMachine, StackMachineConfig};

    /// Test value type for stack machine unit tests.
    #[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
    enum TestValue {
        Unknown,
        Scalar,
        Symbol(u32),
    }

    #[test]
    fn push_and_pop_respect_stack_depth_budget() {
        let mut machine = StackMachine::with_config(
            TestValue::Unknown,
            StackMachineConfig {
                max_stack_depth: Some(2),
                max_locals: None,
                max_symbolic_identities: None,
            },
        );

        machine.push(TestValue::Scalar);
        machine.push(TestValue::Symbol(1));
        machine.push(TestValue::Symbol(2));

        assert_eq!(
            machine.stack_values(),
            &[TestValue::Symbol(1), TestValue::Symbol(2)]
        );
        assert_eq!(machine.pop(), TestValue::Symbol(2));
        assert_eq!(machine.pop(), TestValue::Symbol(1));
        assert_eq!(machine.pop(), TestValue::Unknown);
    }

    #[test]
    fn local_load_and_store_use_defaults_and_budget() {
        let mut machine = StackMachine::with_config(
            TestValue::Unknown,
            StackMachineConfig {
                max_stack_depth: None,
                max_locals: Some(2),
                max_symbolic_identities: None,
            },
        );

        machine.store_local(0, TestValue::Scalar);
        machine.store_local(1, TestValue::Symbol(3));
        machine.store_local(2, TestValue::Symbol(9));

        assert_eq!(machine.load_local(0), TestValue::Scalar);
        assert_eq!(machine.load_local(1), TestValue::Symbol(3));
        assert_eq!(machine.load_local(2), TestValue::Unknown);
    }

    #[test]
    fn canonicalization_and_symbol_cap_normalize_state() {
        let mut machine = StackMachine::with_config(
            TestValue::Unknown,
            StackMachineConfig {
                max_stack_depth: None,
                max_locals: None,
                max_symbolic_identities: Some(2),
            },
        );
        machine.push(TestValue::Symbol(10));
        machine.push(TestValue::Symbol(30));
        machine.push(TestValue::Symbol(20));
        machine.store_local(5, TestValue::Symbol(30));
        machine.store_local(7, TestValue::Symbol(40));

        let tracked = machine
            .enforce_symbolic_identity_cap_u32(
                |value| match value {
                    TestValue::Symbol(id) => Some(*id),
                    _ => None,
                },
                |value| *value = TestValue::Unknown,
            )
            .expect("symbol cap configured");

        assert_eq!(tracked, [30, 40].into_iter().collect());

        machine.canonicalize_symbolic_ids_u32(
            |value| match value {
                TestValue::Symbol(id) => Some(*id),
                _ => None,
            },
            |value, mapped| *value = TestValue::Symbol(mapped),
            std::iter::empty(),
        );

        assert_eq!(
            machine.stack_values(),
            &[TestValue::Unknown, TestValue::Symbol(0), TestValue::Unknown]
        );
        assert_eq!(machine.load_local(5), TestValue::Symbol(0));
        assert_eq!(machine.load_local(7), TestValue::Symbol(1));
    }

    #[test]
    fn canonicalization_produces_deterministic_state_keys() {
        let mut left = StackMachine::new(TestValue::Unknown);
        left.push(TestValue::Symbol(10));
        left.push(TestValue::Symbol(20));
        left.store_local(1, TestValue::Symbol(20));

        let mut right = StackMachine::new(TestValue::Unknown);
        right.push(TestValue::Symbol(100));
        right.push(TestValue::Symbol(200));
        right.store_local(1, TestValue::Symbol(200));

        for machine in [&mut left, &mut right] {
            machine.canonicalize_symbolic_ids_u32(
                |value| match value {
                    TestValue::Symbol(id) => Some(*id),
                    _ => None,
                },
                |value, mapped| *value = TestValue::Symbol(mapped),
                std::iter::empty(),
            );
        }

        assert_eq!(left, right);
    }
}
