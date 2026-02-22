use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::Result;

use crate::ir::{BasicBlock, Instruction, Method};

/// Program-point state tracked by the worklist engine.
pub(crate) trait WorklistState: Clone + Ord {
    fn block_start(&self) -> u32;
    fn instruction_index(&self) -> usize;
    fn set_position(&mut self, block_start: u32, instruction_index: usize);
}

/// Outcome produced after executing transfer for one instruction.
pub(crate) struct InstructionStep<F> {
    findings: Vec<F>,
    terminate_path: bool,
}

impl<F> InstructionStep<F> {
    pub(crate) fn continue_path() -> Self {
        Self {
            findings: Vec::new(),
            terminate_path: false,
        }
    }

    pub(crate) fn terminate_path() -> Self {
        Self {
            findings: Vec::new(),
            terminate_path: true,
        }
    }

    pub(crate) fn with_finding(mut self, finding: F) -> Self {
        self.findings.push(finding);
        self
    }
}

/// Outcome produced when execution reaches the end of a basic block.
pub(crate) struct BlockEndStep<S, F> {
    findings: Vec<F>,
    next_states: Vec<S>,
}

impl<S, F> BlockEndStep<S, F> {
    pub(crate) fn terminal() -> Self {
        Self {
            findings: Vec::new(),
            next_states: Vec::new(),
        }
    }

    pub(crate) fn with_finding(mut self, finding: F) -> Self {
        self.findings.push(finding);
        self
    }
}

impl<S, F> BlockEndStep<S, F>
where
    S: WorklistState,
{
    pub(crate) fn follow_all_successors(state: &S, successors: &[u32]) -> Self {
        let next_states = successors
            .iter()
            .map(|successor| {
                let mut next = state.clone();
                next.set_position(*successor, 0);
                next
            })
            .collect();
        Self {
            findings: Vec::new(),
            next_states,
        }
    }
}

/// Domain callbacks required by the generic worklist engine.
pub(crate) trait WorklistSemantics {
    type State: WorklistState;
    type Finding;

    fn initial_states(&self, method: &Method) -> Vec<Self::State>;

    fn canonicalize_state(&self, _state: &mut Self::State) {}

    fn transfer_instruction(
        &self,
        method: &Method,
        instruction: &Instruction,
        state: &mut Self::State,
    ) -> Result<InstructionStep<Self::Finding>>;

    fn on_block_end(
        &self,
        _method: &Method,
        state: &Self::State,
        successors: &[u32],
    ) -> Result<BlockEndStep<Self::State, Self::Finding>> {
        Ok(BlockEndStep::follow_all_successors(state, successors))
    }
}

/// Deterministic intraprocedural worklist runner for bytecode dataflow analyses.
pub(crate) fn analyze_method<S>(method: &Method, semantics: &S) -> Result<Vec<S::Finding>>
where
    S: WorklistSemantics,
{
    let graph = MethodGraph::new(method);
    let mut queue = VecDeque::new();
    let mut visited = BTreeSet::new();
    let mut findings = Vec::new();

    for mut state in semantics.initial_states(method) {
        semantics.canonicalize_state(&mut state);
        queue.push_back(state);
    }

    while let Some(mut state) = queue.pop_front() {
        semantics.canonicalize_state(&mut state);
        if !visited.insert(state.clone()) {
            continue;
        }

        let Some(block) = graph.blocks.get(&state.block_start()) else {
            continue;
        };

        if state.instruction_index() >= block.instructions.len() {
            let end_step = semantics.on_block_end(
                method,
                &state,
                graph.successors_for(state.block_start()),
            )?;
            enqueue_block_end_step(semantics, end_step, &mut queue, &mut findings);
            continue;
        }

        let instruction = &block.instructions[state.instruction_index()];
        let mut next_state = state.clone();
        next_state.set_position(state.block_start(), state.instruction_index() + 1);

        let step = semantics.transfer_instruction(method, instruction, &mut next_state)?;
        findings.extend(step.findings);
        if step.terminate_path {
            continue;
        }

        semantics.canonicalize_state(&mut next_state);
        let Some(next_block) = graph.blocks.get(&next_state.block_start()) else {
            continue;
        };
        if next_state.instruction_index() < next_block.instructions.len() {
            queue.push_back(next_state);
            continue;
        }

        let end_step = semantics.on_block_end(
            method,
            &next_state,
            graph.successors_for(next_state.block_start()),
        )?;
        enqueue_block_end_step(semantics, end_step, &mut queue, &mut findings);
    }

    Ok(findings)
}

/// CFG lookup tables used by the worklist loop.
struct MethodGraph<'a> {
    blocks: BTreeMap<u32, &'a BasicBlock>,
    successors: BTreeMap<u32, Vec<u32>>,
}

impl<'a> MethodGraph<'a> {
    fn new(method: &'a Method) -> Self {
        let mut blocks = BTreeMap::new();
        for block in &method.cfg.blocks {
            blocks.insert(block.start_offset, block);
        }

        let mut successors: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for block in &method.cfg.blocks {
            successors.entry(block.start_offset).or_default();
        }
        for edge in &method.cfg.edges {
            successors.entry(edge.from).or_default().push(edge.to);
        }
        for targets in successors.values_mut() {
            targets.sort();
            targets.dedup();
        }

        Self { blocks, successors }
    }

    fn successors_for(&self, block_start: u32) -> &[u32] {
        self.successors
            .get(&block_start)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

fn enqueue_block_end_step<S>(
    semantics: &S,
    step: BlockEndStep<S::State, S::Finding>,
    queue: &mut VecDeque<S::State>,
    findings: &mut Vec<S::Finding>,
) where
    S: WorklistSemantics,
{
    findings.extend(step.findings);
    for mut state in step.next_states {
        semantics.canonicalize_state(&mut state);
        queue.push_back(state);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use anyhow::Result;

    use super::{BlockEndStep, InstructionStep, WorklistSemantics, WorklistState, analyze_method};
    use crate::ir::{
        BasicBlock, CallSite, ControlFlowGraph, EdgeKind, FlowEdge, Instruction, InstructionKind,
        LineNumber, LocalVariableType, Method, MethodAccess, MethodNullness, Nullness,
    };

    /// State used by worklist engine tests.
    #[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
    struct TestState {
        block_start: u32,
        instruction_index: usize,
        marker: u8,
    }

    impl WorklistState for TestState {
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

    /// Semantics used to validate single-path traversal.
    struct SinglePathSemantics {
        transfer_calls: Cell<usize>,
    }

    impl WorklistSemantics for SinglePathSemantics {
        type State = TestState;
        type Finding = &'static str;

        fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
            vec![TestState {
                block_start: 0,
                instruction_index: 0,
                marker: 0,
            }]
        }

        fn transfer_instruction(
            &self,
            _method: &Method,
            _instruction: &Instruction,
            _state: &mut Self::State,
        ) -> Result<InstructionStep<Self::Finding>> {
            self.transfer_calls.set(self.transfer_calls.get() + 1);
            Ok(InstructionStep::continue_path())
        }

        fn on_block_end(
            &self,
            _method: &Method,
            state: &Self::State,
            successors: &[u32],
        ) -> Result<BlockEndStep<Self::State, Self::Finding>> {
            if successors.is_empty() {
                return Ok(BlockEndStep::terminal().with_finding("terminal"));
            }
            Ok(BlockEndStep::follow_all_successors(state, successors))
        }
    }

    /// Semantics used to validate merge-point deduplication.
    struct BranchMergeSemantics {
        block_three_visits: Cell<usize>,
    }

    impl WorklistSemantics for BranchMergeSemantics {
        type State = TestState;
        type Finding = ();

        fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
            vec![TestState {
                block_start: 0,
                instruction_index: 0,
                marker: 0,
            }]
        }

        fn transfer_instruction(
            &self,
            _method: &Method,
            instruction: &Instruction,
            state: &mut Self::State,
        ) -> Result<InstructionStep<Self::Finding>> {
            if state.block_start == 1 || state.block_start == 2 {
                state.marker = 1;
            }
            if instruction.offset == 30 {
                self.block_three_visits
                    .set(self.block_three_visits.get() + 1);
            }
            Ok(InstructionStep::continue_path())
        }
    }

    /// Semantics used to validate convergence on loops.
    struct LoopSemantics {
        transfer_calls: Cell<usize>,
    }

    impl WorklistSemantics for LoopSemantics {
        type State = TestState;
        type Finding = ();

        fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
            vec![TestState {
                block_start: 0,
                instruction_index: 0,
                marker: 0,
            }]
        }

        fn transfer_instruction(
            &self,
            _method: &Method,
            _instruction: &Instruction,
            state: &mut Self::State,
        ) -> Result<InstructionStep<Self::Finding>> {
            self.transfer_calls.set(self.transfer_calls.get() + 1);
            state.marker = state.marker.saturating_add(1).min(1);
            Ok(InstructionStep::continue_path())
        }
    }

    /// Semantics used to validate exception-edge traversal.
    struct ExceptionEdgeSemantics;

    impl WorklistSemantics for ExceptionEdgeSemantics {
        type State = TestState;
        type Finding = u32;

        fn initial_states(&self, _method: &Method) -> Vec<Self::State> {
            vec![TestState {
                block_start: 0,
                instruction_index: 0,
                marker: 0,
            }]
        }

        fn transfer_instruction(
            &self,
            _method: &Method,
            instruction: &Instruction,
            _state: &mut Self::State,
        ) -> Result<InstructionStep<Self::Finding>> {
            Ok(InstructionStep::continue_path().with_finding(instruction.offset))
        }
    }

    fn build_method(blocks: Vec<BasicBlock>, edges: Vec<FlowEdge>) -> Method {
        Method {
            name: "MethodX".to_string(),
            descriptor: "()V".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: false,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::Unknown,
                parameter_nullness: Vec::new(),
            },
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::<LineNumber>::new(),
            cfg: ControlFlowGraph { blocks, edges },
            calls: Vec::<CallSite>::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::<LocalVariableType>::new(),
        }
    }

    fn block(start_offset: u32, instruction_offsets: &[u32]) -> BasicBlock {
        BasicBlock {
            start_offset,
            end_offset: start_offset + 1,
            instructions: instruction_offsets
                .iter()
                .map(|offset| Instruction {
                    offset: *offset,
                    opcode: 0,
                    kind: InstructionKind::Other(0),
                })
                .collect(),
        }
    }

    #[test]
    fn traverses_single_path() {
        let method = build_method(vec![block(0, &[0, 1])], Vec::new());
        let semantics = SinglePathSemantics {
            transfer_calls: Cell::new(0),
        };

        let findings = analyze_method(&method, &semantics).expect("worklist run");

        assert_eq!(semantics.transfer_calls.get(), 2);
        assert_eq!(findings, vec!["terminal"]);
    }

    #[test]
    fn deduplicates_merge_state() {
        let method = build_method(
            vec![
                block(0, &[0]),
                block(1, &[10]),
                block(2, &[20]),
                block(3, &[30]),
            ],
            vec![
                FlowEdge {
                    from: 0,
                    to: 1,
                    kind: EdgeKind::Branch,
                },
                FlowEdge {
                    from: 0,
                    to: 2,
                    kind: EdgeKind::FallThrough,
                },
                FlowEdge {
                    from: 1,
                    to: 3,
                    kind: EdgeKind::FallThrough,
                },
                FlowEdge {
                    from: 2,
                    to: 3,
                    kind: EdgeKind::FallThrough,
                },
            ],
        );
        let semantics = BranchMergeSemantics {
            block_three_visits: Cell::new(0),
        };

        analyze_method(&method, &semantics).expect("worklist run");

        assert_eq!(semantics.block_three_visits.get(), 1);
    }

    #[test]
    fn converges_on_loop() {
        let method = build_method(
            vec![block(0, &[0])],
            vec![FlowEdge {
                from: 0,
                to: 0,
                kind: EdgeKind::Branch,
            }],
        );
        let semantics = LoopSemantics {
            transfer_calls: Cell::new(0),
        };

        analyze_method(&method, &semantics).expect("worklist run");

        assert_eq!(semantics.transfer_calls.get(), 2);
    }

    #[test]
    fn traverses_exception_edge() {
        let method = build_method(
            vec![block(0, &[0]), block(1, &[10]), block(2, &[20])],
            vec![
                FlowEdge {
                    from: 0,
                    to: 1,
                    kind: EdgeKind::FallThrough,
                },
                FlowEdge {
                    from: 0,
                    to: 2,
                    kind: EdgeKind::Exception,
                },
            ],
        );

        let findings = analyze_method(&method, &ExceptionEdgeSemantics).expect("worklist run");

        assert!(findings.contains(&20), "expected handler block traversal");
    }
}
