use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use anyhow::Result;
use opentelemetry::KeyValue;
use serde_sarif::sarif::Result as SarifResult;

use crate::descriptor::{
    MethodDescriptorSummary, ReturnKind, method_descriptor_summary, method_param_count,
};
use crate::engine::AnalysisContext;
use crate::ir::{CallKind, Class, ClassTypeUse, Method, Nullness, TypeUse, TypeUseKind};
use crate::opcodes;
use crate::rules::{Rule, RuleMetadata, method_location_with_line, result_message};

// TODO: refer Checkerframework stubs or something like it to handle nullness of standard APIs

/// Rule that will enforce JSpecify-guided nullness checks.
#[derive(Default)]
pub(crate) struct NullnessRule;

crate::register_rule!(NullnessRule);

impl Rule for NullnessRule {
    fn metadata(&self) -> RuleMetadata {
        RuleMetadata {
            id: "NULLNESS",
            name: "Nullness checks",
            description: "Nullness issues guided by JSpecify annotations",
        }
    }

    fn run(&self, context: &AnalysisContext) -> Result<Vec<SarifResult>> {
        let mut results = Vec::new();
        let mut class_map = BTreeMap::new();
        for class in context.all_classes() {
            class_map.insert(class.name.clone(), class);
        }

        for class in context.analysis_target_classes() {
            let mut attributes = vec![KeyValue::new("inspequte.class", class.name.clone())];
            if let Some(uri) = context.class_artifact_uri(class) {
                attributes.push(KeyValue::new("inspequte.artifact_uri", uri));
            }
            let class_results =
                context.with_span("class", &attributes, || -> Result<Vec<SarifResult>> {
                    let mut class_results = Vec::new();
                    let artifact_uri = context.class_artifact_uri(class);
                    class_results.extend(check_overrides(class, &class_map));
                    for method in &class.methods {
                        if method.bytecode.is_empty() {
                            continue;
                        }
                        class_results.extend(check_method_flow(
                            class,
                            method,
                            &class_map,
                            artifact_uri.as_deref(),
                        )?);
                    }
                    Ok(class_results)
                })?;
            results.extend(class_results);
        }

        Ok(deduplicate_results(results))
    }
}

fn deduplicate_results(results: Vec<SarifResult>) -> Vec<SarifResult> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::with_capacity(results.len());
    for result in results {
        let key = result_dedup_key(&result);
        if seen.insert(key) {
            deduped.push(result);
        }
    }
    deduped
}

fn result_dedup_key(result: &SarifResult) -> (String, String, i64, String) {
    let message = result.message.text.clone().unwrap_or_default();
    let (artifact_uri, line, logical) = if let Some(location) = result
        .locations
        .as_ref()
        .and_then(|locations| locations.first())
    {
        let artifact_uri = location
            .physical_location
            .as_ref()
            .and_then(|physical| physical.artifact_location.as_ref())
            .and_then(|artifact| artifact.uri.clone())
            .unwrap_or_default();
        let line = location
            .physical_location
            .as_ref()
            .and_then(|physical| physical.region.as_ref())
            .and_then(|region| region.start_line)
            .unwrap_or(0);
        let logical = location
            .logical_locations
            .as_ref()
            .and_then(|logicals| logicals.first())
            .and_then(|logical| logical.name.clone())
            .unwrap_or_default();
        (artifact_uri, line, logical)
    } else {
        (String::new(), 0, String::new())
    };
    (message, artifact_uri, line, logical)
}

fn check_overrides(class: &Class, class_map: &BTreeMap<String, &Class>) -> Vec<SarifResult> {
    let mut results = Vec::new();
    let supertypes = collect_supertypes(class, class_map);
    for method in &class.methods {
        for super_class in &supertypes {
            let Some(base_method) = find_method(super_class, &method.name, &method.descriptor)
            else {
                continue;
            };
            if base_method.type_use.is_some() && method.type_use.is_some() {
                // Type-use metadata covers top-level nullness too, so avoid duplicate reports.
                results.extend(check_type_use_overrides(
                    class,
                    method,
                    base_method,
                    method_location_with_line(
                        &class.name,
                        &method.name,
                        &method.descriptor,
                        None,
                        None,
                    ),
                ));
            } else {
                results.extend(check_signature_overrides(class, method, base_method));
            }
        }
    }
    results
}

fn check_signature_overrides(
    class: &Class,
    method: &Method,
    base_method: &Method,
) -> Vec<SarifResult> {
    let mut results = Vec::new();
    if base_method.nullness.return_nullness == Nullness::NonNull
        && method.nullness.return_nullness == Nullness::Nullable
    {
        let message = result_message(format!(
            "Nullness override: {}.{}{} returns @Nullable but overrides @NonNull",
            class.name, method.name, method.descriptor
        ));
        let location =
            method_location_with_line(&class.name, &method.name, &method.descriptor, None, None);
        results.push(
            SarifResult::builder()
                .message(message)
                .locations(vec![location])
                .build(),
        );
    }
    let param_count = method.nullness.parameter_nullness.len();
    let base_param_count = base_method.nullness.parameter_nullness.len();
    let count = param_count.min(base_param_count);
    for index in 0..count {
        if base_method.nullness.parameter_nullness[index] == Nullness::Nullable
            && method.nullness.parameter_nullness[index] == Nullness::NonNull
        {
            let message = result_message(format!(
                "Nullness override: {}.{}{} parameter {} is @NonNull but overrides @Nullable",
                class.name, method.name, method.descriptor, index
            ));
            let location = method_location_with_line(
                &class.name,
                &method.name,
                &method.descriptor,
                None,
                None,
            );
            results.push(
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location])
                    .build(),
            );
        }
    }
    results
}

fn check_type_use_overrides(
    class: &Class,
    method: &Method,
    base_method: &Method,
    location: serde_sarif::sarif::Location,
) -> Vec<SarifResult> {
    let mut results = Vec::new();
    let Some(base_type_use) = base_method.type_use.as_ref() else {
        return results;
    };
    let Some(method_type_use) = method.type_use.as_ref() else {
        return results;
    };
    if let (Some(base_return), Some(method_return)) = (
        base_type_use.return_type.as_ref(),
        method_type_use.return_type.as_ref(),
    ) {
        if type_use_override_conflict(base_return, method_return, TypeUseVariance::Return) {
            let message = result_message(format!(
                "Nullness override: {}.{}{} return type-use is more nullable than the overridden method; consider marking the override return type (or nested type argument) @NonNull or relaxing the base signature to @Nullable",
                class.name, method.name, method.descriptor
            ));
            results.push(
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location.clone()])
                    .build(),
            );
        }
    }
    let count = base_type_use
        .parameters
        .len()
        .min(method_type_use.parameters.len());
    for index in 0..count {
        if type_use_override_conflict(
            &base_type_use.parameters[index],
            &method_type_use.parameters[index],
            TypeUseVariance::Parameter,
        ) {
            let message = result_message(format!(
                "Nullness override: {}.{}{} parameter {} has a more restrictive (less nullable) type-use than the overridden method at this parameter or one of its nested type arguments (for example, base @Nullable vs override @NonNull); consider marking the override parameter or the conflicting nested type argument @Nullable, or tightening the corresponding base signature location to @NonNull",
                class.name, method.name, method.descriptor, index
            ));
            results.push(
                SarifResult::builder()
                    .message(message)
                    .locations(vec![location.clone()])
                    .build(),
            );
        }
    }
    results
}

#[derive(Copy, Clone)]
enum TypeUseVariance {
    Return,
    Parameter,
    Invariant,
}

fn type_use_override_conflict(
    base: &TypeUse,
    derived: &TypeUse,
    variance: TypeUseVariance,
) -> bool {
    if type_use_nullness_conflict(base.nullness, derived.nullness, variance) {
        return true;
    }
    match (&base.kind, &derived.kind) {
        (TypeUseKind::Array(base), TypeUseKind::Array(derived)) => {
            type_use_override_conflict(base, derived, variance)
        }
        (TypeUseKind::Class(base), TypeUseKind::Class(derived)) => {
            class_type_use_conflict(base, derived, variance)
        }
        (TypeUseKind::Wildcard(base), TypeUseKind::Wildcard(derived)) => {
            match (base.as_deref(), derived.as_deref()) {
                (Some(base), Some(derived)) => type_use_override_conflict(base, derived, variance),
                _ => false,
            }
        }
        (TypeUseKind::TypeVar(_), TypeUseKind::TypeVar(_))
        | (TypeUseKind::Base(_), TypeUseKind::Base(_))
        | (TypeUseKind::Void, TypeUseKind::Void) => false,
        _ => false,
    }
}

fn type_use_nullness_conflict(
    base: Nullness,
    derived: Nullness,
    variance: TypeUseVariance,
) -> bool {
    match variance {
        TypeUseVariance::Return => base == Nullness::NonNull && derived == Nullness::Nullable,
        TypeUseVariance::Parameter => base == Nullness::Nullable && derived == Nullness::NonNull,
        TypeUseVariance::Invariant => {
            matches!(
                (base, derived),
                (Nullness::NonNull, Nullness::Nullable) | (Nullness::Nullable, Nullness::NonNull)
            )
        }
    }
}

fn class_type_use_conflict(
    base: &ClassTypeUse,
    derived: &ClassTypeUse,
    variance: TypeUseVariance,
) -> bool {
    if base.name != derived.name {
        return false;
    }
    if base.type_arguments.len() != derived.type_arguments.len() {
        return false;
    }
    // Java generic type arguments are invariant, so compare nullness strictly.
    for (base_arg, derived_arg) in base
        .type_arguments
        .iter()
        .zip(derived.type_arguments.iter())
    {
        if type_use_override_conflict(base_arg, derived_arg, TypeUseVariance::Invariant) {
            return true;
        }
    }
    match (base.inner.as_deref(), derived.inner.as_deref()) {
        (Some(base_inner), Some(derived_inner)) => {
            type_use_override_conflict(base_inner, derived_inner, variance)
        }
        (None, None) => false,
        _ => false,
    }
}

fn collect_supertypes<'a>(
    class: &'a Class,
    class_map: &'a BTreeMap<String, &'a Class>,
) -> Vec<&'a Class> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::new();
    if let Some(super_name) = &class.super_name {
        queue.push_back(super_name.clone());
    }
    for interface in &class.interfaces {
        queue.push_back(interface.clone());
    }
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(super_class) = class_map.get(&name) else {
            continue;
        };
        result.push(*super_class);
        if let Some(super_name) = &super_class.super_name {
            queue.push_back(super_name.clone());
        }
        for interface in &super_class.interfaces {
            queue.push_back(interface.clone());
        }
    }
    result
}

fn find_method<'a>(class: &'a Class, name: &str, descriptor: &str) -> Option<&'a Method> {
    class
        .methods
        .iter()
        .find(|method| method.name == name && method.descriptor == descriptor)
}

fn check_method_flow(
    class: &Class,
    method: &Method,
    class_map: &BTreeMap<String, &Class>,
    artifact_uri: Option<&str>,
) -> Result<Vec<SarifResult>> {
    let mut results = Vec::new();
    let call_infos = build_method_call_infos(method, class_map)?;
    let call_index_by_offset = build_callsite_index_by_offset(method, &call_infos);

    let local_count = local_count(method)?;
    let mut initial_locals = vec![Nullness::Unknown; local_count];
    let mut initial_local_type_use = vec![None; local_count];
    if !method.access.is_static && !initial_locals.is_empty() {
        initial_locals[0] = Nullness::NonNull;
        initial_local_type_use[0] = Some(this_type_use(class));
    }
    let base_index = if method.access.is_static { 0 } else { 1 };
    for (index, nullness) in method.nullness.parameter_nullness.iter().enumerate() {
        let local_index = base_index + index;
        if let Some(local) = initial_locals.get_mut(local_index) {
            *local = *nullness;
        }
    }
    if let Some(method_type_use) = method.type_use.as_ref() {
        for (index, parameter) in method_type_use.parameters.iter().enumerate() {
            let local_index = base_index + index;
            if let Some(local) = initial_local_type_use.get_mut(local_index) {
                *local = Some(parameter.clone());
            }
        }
    }
    let entry_state = State {
        locals: initial_locals,
        local_type_use: initial_local_type_use,
        stack: Vec::new(),
    };

    let mut block_map = BTreeMap::new();
    for block in &method.cfg.blocks {
        block_map.insert(block.start_offset, block);
    }
    let mut predecessors: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    let mut successors: BTreeMap<u32, Vec<(u32, crate::ir::EdgeKind)>> = BTreeMap::new();
    for edge in &method.cfg.edges {
        predecessors.entry(edge.to).or_default().push(edge.from);
        successors
            .entry(edge.from)
            .or_default()
            .push((edge.to, edge.kind));
    }

    let mut in_states: BTreeMap<u32, State> = BTreeMap::new();
    let mut out_states: BTreeMap<u32, State> = BTreeMap::new();
    let mut worklist = VecDeque::new();
    if block_map.contains_key(&0) {
        in_states.insert(0, entry_state.clone());
        worklist.push_back(0);
    }

    while let Some(block_start) = worklist.pop_front() {
        let Some(block) = block_map.get(&block_start) else {
            continue;
        };
        let in_state = match predecessors.get(&block_start) {
            Some(preds) if block_start != 0 => {
                let mut merged: Option<State> = None;
                for pred in preds {
                    if let Some(state) = out_states.get(pred) {
                        merged = Some(match merged {
                            Some(existing) => join_states(&existing, state),
                            None => state.clone(),
                        });
                    }
                }
                merged.unwrap_or_else(|| in_states[&block_start].clone())
            }
            _ => in_states
                .get(&block_start)
                .cloned()
                .unwrap_or_else(|| entry_state.clone()),
        };

        let transfer = transfer_block(
            class,
            method,
            block,
            &in_state,
            &call_infos,
            &call_index_by_offset,
            artifact_uri,
        )?;
        let out_state = transfer.out_state.clone();
        out_states.insert(block_start, out_state.clone());

        if let Some(succs) = successors.get(&block_start) {
            for (succ, kind) in succs {
                let mut next_state = out_state.clone();
                if let Some(refinement) = transfer.branch_refinement.as_ref() {
                    if matches!(kind, crate::ir::EdgeKind::Branch) {
                        next_state = refinement.apply_to(&next_state, BranchKind::Branch);
                    } else if matches!(kind, crate::ir::EdgeKind::FallThrough) {
                        next_state = refinement.apply_to(&next_state, BranchKind::FallThrough);
                    }
                }
                let updated = match in_states.get(succ) {
                    Some(existing) => join_states(existing, &next_state),
                    None => next_state,
                };
                let should_push = match in_states.get(succ) {
                    Some(existing) => &updated != existing,
                    None => true,
                };
                in_states.insert(*succ, updated);
                if should_push {
                    worklist.push_back(*succ);
                }
            }
        }

        results.extend(transfer.results);
    }

    Ok(results)
}

/// Nullness state at a program point.
#[derive(Clone, Debug, PartialEq)]
struct State {
    locals: Vec<Nullness>,
    local_type_use: Vec<Option<TypeUse>>,
    stack: Vec<StackValue>,
}

/// Operand stack entry with optional local aliasing metadata.
#[derive(Clone, Debug, PartialEq)]
struct StackValue {
    nullness: Nullness,
    type_use: Option<TypeUse>,
    local: Option<usize>,
    is_null_literal: bool,
}

/// Descriptor-derived callsite summary cached for flow transfer.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
struct CallDescriptorInfo {
    param_count: usize,
    return_kind: ReturnKind,
}

/// Flow-transfer metadata for a callsite at a concrete bytecode offset.
#[derive(Copy, Clone)]
struct MethodCallInfo<'a> {
    call: &'a crate::ir::CallSite,
    descriptor: CallDescriptorInfo,
    target: Option<ResolvedCallTarget<'a>>,
}

/// Resolved owner/method pair for a callsite.
#[derive(Copy, Clone)]
struct ResolvedCallTarget<'a> {
    class: &'a Class,
    method: &'a Method,
}

/// Transfer output for a basic block, including emitted results.
#[derive(Clone)]
struct BlockTransfer {
    out_state: State,
    branch_refinement: Option<BranchRefinement>,
    results: Vec<SarifResult>,
}

/// Nullness refinement applied on conditional branch edges.
#[derive(Clone)]
struct BranchRefinement {
    local: usize,
    branch_nullness: Nullness,
    fallthrough_nullness: Nullness,
}

enum BranchKind {
    Branch,
    FallThrough,
}

impl BranchRefinement {
    fn apply_to(&self, state: &State, kind: BranchKind) -> State {
        let mut next = state.clone();
        let value = match kind {
            BranchKind::Branch => self.branch_nullness,
            BranchKind::FallThrough => self.fallthrough_nullness,
        };
        if let Some(local) = next.locals.get_mut(self.local) {
            *local = value;
        }
        next
    }
}

fn build_method_call_infos<'a>(
    method: &'a Method,
    class_map: &'a BTreeMap<String, &'a Class>,
) -> Result<Vec<MethodCallInfo<'a>>> {
    let mut infos = Vec::with_capacity(method.calls.len());
    let mut descriptor_cache: HashMap<&str, CallDescriptorInfo> = HashMap::new();
    for call in &method.calls {
        let descriptor = if let Some(summary) = descriptor_cache.get(call.descriptor.as_str()) {
            *summary
        } else {
            let MethodDescriptorSummary {
                param_count,
                return_kind,
            } = method_descriptor_summary(&call.descriptor)?;
            let summary = CallDescriptorInfo {
                param_count,
                return_kind,
            };
            descriptor_cache.insert(call.descriptor.as_str(), summary);
            summary
        };
        infos.push(MethodCallInfo {
            call,
            descriptor,
            target: resolve_call_target(class_map, call),
        });
    }
    Ok(infos)
}

fn build_callsite_index_by_offset(
    method: &Method,
    call_infos: &[MethodCallInfo<'_>],
) -> Vec<Option<usize>> {
    let mut call_index_by_offset = vec![None; method.bytecode.len()];
    for (index, info) in call_infos.iter().enumerate() {
        let offset = info.call.offset as usize;
        if let Some(slot) = call_index_by_offset.get_mut(offset) {
            *slot = Some(index);
        }
    }
    call_index_by_offset
}

fn resolve_call_target<'a>(
    class_map: &'a BTreeMap<String, &'a Class>,
    call: &crate::ir::CallSite,
) -> Option<ResolvedCallTarget<'a>> {
    let class = class_map.get(&call.owner).copied()?;
    let method = class
        .methods
        .iter()
        .find(|method| method.name == call.name && method.descriptor == call.descriptor)?;
    Some(ResolvedCallTarget { class, method })
}

fn transfer_block(
    class: &Class,
    method: &Method,
    block: &crate::ir::BasicBlock,
    input: &State,
    method_calls: &[MethodCallInfo<'_>],
    call_index_by_offset: &[Option<usize>],
    artifact_uri: Option<&str>,
) -> Result<BlockTransfer> {
    let mut state = input.clone();
    let mut results = Vec::new();
    let mut branch_refinement = None;
    for inst in &block.instructions {
        match inst.opcode {
            opcodes::ACONST_NULL => {
                state.stack.push(StackValue {
                    nullness: Nullness::Nullable,
                    type_use: None,
                    local: None,
                    is_null_literal: true,
                });
            }
            opcodes::ALOAD => {
                let local_index = method
                    .bytecode
                    .get(inst.offset as usize + 1)
                    .copied()
                    .unwrap_or(0) as usize;
                let nullness = state
                    .locals
                    .get(local_index)
                    .copied()
                    .unwrap_or(Nullness::Unknown);
                let type_use = state.local_type_use.get(local_index).cloned().flatten();
                state.stack.push(StackValue {
                    nullness,
                    type_use,
                    local: Some(local_index),
                    is_null_literal: false,
                });
            }
            opcodes::ALOAD_0 | opcodes::ALOAD_1 | opcodes::ALOAD_2 | opcodes::ALOAD_3 => {
                let local_index = (inst.opcode - opcodes::ALOAD_0) as usize;
                let nullness = state
                    .locals
                    .get(local_index)
                    .copied()
                    .unwrap_or(Nullness::Unknown);
                let type_use = state.local_type_use.get(local_index).cloned().flatten();
                state.stack.push(StackValue {
                    nullness,
                    type_use,
                    local: Some(local_index),
                    is_null_literal: false,
                });
            }
            opcodes::ASTORE => {
                let local_index = method
                    .bytecode
                    .get(inst.offset as usize + 1)
                    .copied()
                    .unwrap_or(0) as usize;
                let value = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                if let Some(local) = state.locals.get_mut(local_index) {
                    *local = value.nullness;
                }
                if let Some(local_type_use) = state.local_type_use.get_mut(local_index) {
                    *local_type_use = value.type_use;
                }
            }
            opcodes::ASTORE_0 | opcodes::ASTORE_1 | opcodes::ASTORE_2 | opcodes::ASTORE_3 => {
                let local_index = (inst.opcode - opcodes::ASTORE_0) as usize;
                let value = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                if let Some(local) = state.locals.get_mut(local_index) {
                    *local = value.nullness;
                }
                if let Some(local_type_use) = state.local_type_use.get_mut(local_index) {
                    *local_type_use = value.type_use;
                }
            }
            opcodes::POP => {
                state.stack.pop();
            }
            opcodes::DUP => {
                if let Some(top) = state.stack.last().cloned() {
                    state.stack.push(top);
                }
            }
            opcodes::NEW => {
                state.stack.push(StackValue {
                    nullness: Nullness::NonNull,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
            }
            opcodes::IFNULL | opcodes::IFNONNULL => {
                let value = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                if let Some(local) = value.local {
                    let (branch_nullness, fallthrough_nullness) = if inst.opcode == opcodes::IFNULL
                    {
                        (Nullness::Nullable, Nullness::NonNull)
                    } else {
                        (Nullness::NonNull, Nullness::Nullable)
                    };
                    branch_refinement = Some(BranchRefinement {
                        local,
                        branch_nullness,
                        fallthrough_nullness,
                    });
                    if let Some(local_ref) = state.locals.get_mut(local) {
                        *local_ref = fallthrough_nullness;
                    }
                }
            }
            opcodes::IF_ACMPEQ | opcodes::IF_ACMPNE => {
                let right = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                let left = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                let (local, null_literal) = if left.local.is_some() && right.is_null_literal {
                    (left.local, true)
                } else if right.local.is_some() && left.is_null_literal {
                    (right.local, true)
                } else {
                    (None, false)
                };
                if null_literal {
                    if let Some(local) = local {
                        let (branch_nullness, fallthrough_nullness) =
                            if inst.opcode == opcodes::IF_ACMPEQ {
                                (Nullness::Nullable, Nullness::NonNull)
                            } else {
                                (Nullness::NonNull, Nullness::Nullable)
                            };
                        branch_refinement = Some(BranchRefinement {
                            local,
                            branch_nullness,
                            fallthrough_nullness,
                        });
                        if let Some(local_ref) = state.locals.get_mut(local) {
                            *local_ref = fallthrough_nullness;
                        }
                    }
                }
            }
            opcodes::INVOKEVIRTUAL
            | opcodes::INVOKEINTERFACE
            | opcodes::INVOKESPECIAL
            | opcodes::INVOKESTATIC => {
                let call_info = call_index_by_offset
                    .get(inst.offset as usize)
                    .and_then(|index| index.and_then(|i| method_calls.get(i)));
                if let Some(call_info) = call_info {
                    for _ in 0..call_info.descriptor.param_count {
                        state.stack.pop();
                    }
                    let receiver = if call_info.call.kind != CallKind::Static {
                        Some(state.stack.pop().unwrap_or(StackValue {
                            nullness: Nullness::Unknown,
                            type_use: None,
                            local: None,
                            is_null_literal: false,
                        }))
                    } else {
                        None
                    };
                    if let Some(receiver) = receiver.as_ref() {
                        if receiver.nullness == Nullness::Nullable {
                            let message = result_message(format!(
                                "Nullness issue: possible null receiver in call to {}.{}{}",
                                call_info.call.owner,
                                call_info.call.name,
                                call_info.call.descriptor
                            ));
                            let line = method.line_for_offset(inst.offset);
                            let location = method_location_with_line(
                                &class.name,
                                &method.name,
                                &method.descriptor,
                                artifact_uri,
                                line,
                            );
                            results.push(
                                SarifResult::builder()
                                    .message(message)
                                    .locations(vec![location])
                                    .build(),
                            );
                        }
                    }
                    if call_info.descriptor.return_kind == ReturnKind::Reference {
                        let (return_nullness, return_type_use) = lookup_return_value(
                            call_info.target.as_ref(),
                            call_info.call.kind,
                            receiver.as_ref(),
                        );
                        state.stack.push(StackValue {
                            nullness: return_nullness,
                            type_use: return_type_use,
                            local: None,
                            is_null_literal: false,
                        });
                    }
                }
            }
            opcodes::ARETURN => {
                let value = state.stack.pop().unwrap_or(StackValue {
                    nullness: Nullness::Unknown,
                    type_use: None,
                    local: None,
                    is_null_literal: false,
                });
                if method.nullness.return_nullness == Nullness::NonNull
                    && value.nullness == Nullness::Nullable
                {
                    let message = result_message(format!(
                        "Nullness issue: {}.{}{} returns null but is @NonNull",
                        class.name, method.name, method.descriptor
                    ));
                    let line = method.line_for_offset(inst.offset);
                    let location = method_location_with_line(
                        &class.name,
                        &method.name,
                        &method.descriptor,
                        artifact_uri,
                        line,
                    );
                    results.push(
                        SarifResult::builder()
                            .message(message)
                            .locations(vec![location])
                            .build(),
                    );
                }
            }
            _ => {}
        }
    }

    Ok(BlockTransfer {
        out_state: state,
        branch_refinement,
        results,
    })
}

fn local_count(method: &Method) -> Result<usize> {
    let mut max_index = 0usize;
    let mut offset = 0usize;
    while offset < method.bytecode.len() {
        let opcode = method.bytecode[offset];
        if opcode == opcodes::ALOAD || opcode == opcodes::ASTORE {
            if let Some(index) = method.bytecode.get(offset + 1) {
                max_index = max_index.max(*index as usize);
            }
        } else if opcode >= opcodes::ALOAD_0 && opcode <= opcodes::ALOAD_3 {
            max_index = max_index.max((opcode - opcodes::ALOAD_0) as usize);
        } else if opcode >= opcodes::ASTORE_0 && opcode <= opcodes::ASTORE_3 {
            max_index = max_index.max((opcode - opcodes::ASTORE_0) as usize);
        }
        let length = crate::scan::opcode_length(&method.bytecode, offset)?;
        offset += length;
    }
    let param_count = method_param_count(&method.descriptor)?;
    let base = if method.access.is_static { 0 } else { 1 };
    Ok(max_index.max(base + param_count).saturating_add(1))
}

fn join_states(left: &State, right: &State) -> State {
    let max_locals = left.locals.len().max(right.locals.len());
    let mut locals = Vec::with_capacity(max_locals);
    let mut local_type_use = Vec::with_capacity(max_locals);
    for index in 0..max_locals {
        let l = left.locals.get(index).copied().unwrap_or(Nullness::Unknown);
        let r = right
            .locals
            .get(index)
            .copied()
            .unwrap_or(Nullness::Unknown);
        locals.push(join_nullness(l, r));
        local_type_use.push(join_type_use(
            left.local_type_use.get(index).and_then(Option::as_ref),
            right.local_type_use.get(index).and_then(Option::as_ref),
        ));
    }
    let stack = if left.stack.len() == right.stack.len() {
        left.stack
            .iter()
            .zip(right.stack.iter())
            .map(|(l, r)| StackValue {
                nullness: join_nullness(l.nullness, r.nullness),
                type_use: join_type_use(l.type_use.as_ref(), r.type_use.as_ref()),
                local: if l.local == r.local { l.local } else { None },
                is_null_literal: l.is_null_literal && r.is_null_literal,
            })
            .collect()
    } else {
        Vec::new()
    };
    State {
        locals,
        local_type_use,
        stack,
    }
}

fn join_nullness(left: Nullness, right: Nullness) -> Nullness {
    match (left, right) {
        (Nullness::NonNull, Nullness::NonNull) => Nullness::NonNull,
        (Nullness::Nullable, Nullness::Nullable) => Nullness::Nullable,
        _ => Nullness::Unknown,
    }
}

fn join_type_use(left: Option<&TypeUse>, right: Option<&TypeUse>) -> Option<TypeUse> {
    if left == right {
        return left.cloned();
    }
    None
}

fn this_type_use(class: &Class) -> TypeUse {
    TypeUse {
        nullness: Nullness::NonNull,
        kind: TypeUseKind::Class(ClassTypeUse {
            name: class.name.clone(),
            type_arguments: class
                .type_parameters
                .iter()
                .map(|parameter| TypeUse {
                    nullness: Nullness::Unknown,
                    kind: TypeUseKind::TypeVar(parameter.name.clone()),
                })
                .collect(),
            inner: None,
        }),
    }
}

fn lookup_return_value(
    target: Option<&ResolvedCallTarget<'_>>,
    call_kind: CallKind,
    receiver: Option<&StackValue>,
) -> (Nullness, Option<TypeUse>) {
    let Some(target) = target else {
        return (Nullness::Unknown, None);
    };
    let class = target.class;
    let method = target.method;

    let mut return_type_use = method
        .type_use
        .as_ref()
        .and_then(|method_type_use| method_type_use.return_type.clone());
    let mut unresolved_top_level_type_variable = false;
    if let Some(ref mut return_type) = return_type_use {
        if call_kind != CallKind::Static {
            let substitutions = receiver
                .and_then(|stack_value| stack_value.type_use.as_ref())
                .map(|type_use| receiver_type_variable_substitutions(class, type_use))
                .unwrap_or_default();
            let (substituted, unresolved) = substitute_type_variables(return_type, &substitutions);
            *return_type = substituted;
            unresolved_top_level_type_variable = unresolved;
        }
    }
    let return_nullness = match return_type_use.as_ref() {
        Some(return_type)
            if unresolved_top_level_type_variable
                && matches!(return_type.kind, TypeUseKind::TypeVar(_)) =>
        {
            Nullness::Unknown
        }
        Some(return_type) => match return_type.nullness {
            Nullness::Unknown => method.nullness.return_nullness,
            value => value,
        },
        None => method.nullness.return_nullness,
    };
    (return_nullness, return_type_use)
}

fn receiver_type_variable_substitutions(
    class: &Class,
    receiver_type_use: &TypeUse,
) -> BTreeMap<String, TypeUse> {
    let Some(receiver_class_type_use) =
        find_class_type_use_for_owner(receiver_type_use, &class.name)
    else {
        return BTreeMap::new();
    };
    class
        .type_parameters
        .iter()
        .zip(receiver_class_type_use.type_arguments.iter())
        .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
        .collect()
}

fn find_class_type_use_for_owner<'a>(
    type_use: &'a TypeUse,
    owner: &str,
) -> Option<&'a ClassTypeUse> {
    let TypeUseKind::Class(class_type_use) = &type_use.kind else {
        return None;
    };
    if class_type_use.name == owner {
        return Some(class_type_use);
    }
    find_inner_class_type_use(class_type_use.inner.as_deref(), owner)
}

fn find_inner_class_type_use<'a>(
    inner: Option<&'a TypeUse>,
    owner: &str,
) -> Option<&'a ClassTypeUse> {
    let Some(inner) = inner else {
        return None;
    };
    let TypeUseKind::Class(class_type_use) = &inner.kind else {
        return None;
    };
    if class_type_use.name == owner {
        return Some(class_type_use);
    }
    find_inner_class_type_use(class_type_use.inner.as_deref(), owner)
}

fn substitute_type_variables(
    type_use: &TypeUse,
    substitutions: &BTreeMap<String, TypeUse>,
) -> (TypeUse, bool) {
    match &type_use.kind {
        TypeUseKind::TypeVar(name) => {
            if let Some(mapped) = substitutions.get(name) {
                let mut resolved = mapped.clone();
                if type_use.nullness == Nullness::Nullable {
                    resolved.nullness = Nullness::Nullable;
                }
                return (resolved, false);
            }
            return (type_use.clone(), true);
        }
        TypeUseKind::Array(component) => {
            let (substituted, _) = substitute_type_variables(component, substitutions);
            (
                TypeUse {
                    nullness: type_use.nullness,
                    kind: TypeUseKind::Array(Box::new(substituted)),
                },
                false,
            )
        }
        TypeUseKind::Class(class_type_use) => {
            let type_arguments = class_type_use
                .type_arguments
                .iter()
                .map(|argument| substitute_type_variables(argument, substitutions).0)
                .collect();
            let inner = class_type_use.inner.as_ref().map(|inner| {
                let (substituted, _) = substitute_type_variables(inner, substitutions);
                Box::new(substituted)
            });
            (
                TypeUse {
                    nullness: type_use.nullness,
                    kind: TypeUseKind::Class(ClassTypeUse {
                        name: class_type_use.name.clone(),
                        type_arguments,
                        inner,
                    }),
                },
                false,
            )
        }
        TypeUseKind::Wildcard(Some(bound)) => {
            let (substituted, _) = substitute_type_variables(bound, substitutions);
            (
                TypeUse {
                    nullness: type_use.nullness,
                    kind: TypeUseKind::Wildcard(Some(Box::new(substituted))),
                },
                false,
            )
        }
        TypeUseKind::Wildcard(None) | TypeUseKind::Base(_) | TypeUseKind::Void => {
            (type_use.clone(), false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::build_context;
    use crate::ir::{
        BasicBlock, CallKind, CallSite, Class, ControlFlowGraph, Instruction, InstructionKind,
        MethodAccess, MethodNullness, MethodTypeUse, TypeParameterUse,
    };
    use crate::test_harness::{JvmTestHarness, Language, SourceFile};

    fn method_with(
        name: &str,
        descriptor: &str,
        access: MethodAccess,
        nullness: MethodNullness,
        bytecode: Vec<u8>,
        instructions: Vec<Instruction>,
        calls: Vec<CallSite>,
    ) -> Method {
        let end_offset = bytecode.len() as u32;
        Method {
            name: name.to_string(),
            descriptor: descriptor.to_string(),
            signature: None,
            access,
            nullness,
            type_use: None,
            bytecode,
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: vec![BasicBlock {
                    start_offset: 0,
                    end_offset,
                    instructions,
                }],
                edges: Vec::new(),
            },
            calls,
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        }
    }

    fn class_with_methods(name: &str, super_name: Option<&str>, methods: Vec<Method>) -> Class {
        Class {
            name: name.to_string(),
            source_file: None,
            super_name: super_name.map(str::to_string),
            interfaces: Vec::new(),
            type_parameters: Vec::new(),
            referenced_classes: Vec::new(),
            fields: Vec::new(),
            methods,
            annotation_defaults: Vec::new(),
            artifact_index: 0,
            is_record: false,
        }
    }

    fn context_for(classes: Vec<Class>) -> AnalysisContext {
        build_context(classes, &[])
    }

    fn jspecify_stubs() -> Vec<SourceFile> {
        vec![
            SourceFile {
                path: "org/jspecify/annotations/NullMarked.java".to_string(),
                contents: r#"
package org.jspecify.annotations;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.PACKAGE})
public @interface NullMarked {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/jspecify/annotations/NullUnmarked.java".to_string(),
                contents: r#"
package org.jspecify.annotations;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE, ElementType.PACKAGE})
public @interface NullUnmarked {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/jspecify/annotations/Nullable.java".to_string(),
                contents: r#"
package org.jspecify.annotations;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
public @interface Nullable {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/jspecify/annotations/NonNull.java".to_string(),
                contents: r#"
package org.jspecify.annotations;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
public @interface NonNull {}
"#
                .to_string(),
            },
            SourceFile {
                path: "org/jspecify/annotations/NullnessUnspecified.java".to_string(),
                contents: r#"
package org.jspecify.annotations;
import java.lang.annotation.ElementType;
import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;
import java.lang.annotation.Target;
@Retention(RetentionPolicy.RUNTIME)
@Target({ElementType.TYPE_USE, ElementType.TYPE_PARAMETER})
public @interface NullnessUnspecified {}
"#
                .to_string(),
            },
        ]
    }

    fn analyze_with_harness(sources: Vec<SourceFile>) -> crate::engine::EngineOutput {
        let harness = JvmTestHarness::new().expect("JAVA_HOME must be set for harness tests");
        harness
            .compile_and_analyze(Language::Java, &sources, &[])
            .expect("run harness analysis")
    }

    #[test]
    fn deduplicate_results_removes_identical_result_entries() {
        let duplicate_a = SarifResult::builder()
            .message(result_message(
                "Nullness issue: possible null receiver in call to a/b/C.m()V",
            ))
            .locations(vec![crate::rules::method_location_with_line(
                "a/b/C",
                "methodX",
                "()V",
                Some("file:///tmp/C.class"),
                Some(42),
            )])
            .build();
        let duplicate_b = SarifResult::builder()
            .message(result_message(
                "Nullness issue: possible null receiver in call to a/b/C.m()V",
            ))
            .locations(vec![crate::rules::method_location_with_line(
                "a/b/C",
                "methodX",
                "()V",
                Some("file:///tmp/C.class"),
                Some(42),
            )])
            .build();
        let distinct = SarifResult::builder()
            .message(result_message(
                "Nullness issue: possible null receiver in call to a/b/C.m()V",
            ))
            .locations(vec![crate::rules::method_location_with_line(
                "a/b/C",
                "methodX",
                "()V",
                Some("file:///tmp/C.class"),
                Some(43),
            )])
            .build();

        let deduped = deduplicate_results(vec![duplicate_a, duplicate_b, distinct]);

        assert_eq!(2, deduped.len());
    }

    #[test]
    fn nullness_override_reports_return_mismatch() {
        let base_method = Method {
            name: "value".to_string(),
            descriptor: "()Ljava/lang/String;".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::NonNull,
                parameter_nullness: Vec::new(),
            },
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        };
        let override_method = Method {
            name: "value".to_string(),
            descriptor: "()Ljava/lang/String;".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::Nullable,
                parameter_nullness: Vec::new(),
            },
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        };
        let base = class_with_methods("com/example/Base", None, vec![base_method]);
        let derived = class_with_methods(
            "com/example/Derived",
            Some("com/example/Base"),
            vec![override_method],
        );
        let context = context_for(vec![base, derived]);

        let results = NullnessRule.run(&context).expect("nullness rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("returns @Nullable but overrides @NonNull"));
    }

    #[test]
    fn nullness_override_reports_parameter_mismatch() {
        let base_method = Method {
            name: "set".to_string(),
            descriptor: "(Ljava/lang/String;)V".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::Unknown,
                parameter_nullness: vec![Nullness::Nullable],
            },
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        };
        let override_method = Method {
            name: "set".to_string(),
            descriptor: "(Ljava/lang/String;)V".to_string(),
            signature: None,
            access: MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            nullness: MethodNullness {
                return_nullness: Nullness::Unknown,
                parameter_nullness: vec![Nullness::NonNull],
            },
            type_use: None,
            bytecode: Vec::new(),
            line_numbers: Vec::new(),
            cfg: ControlFlowGraph {
                blocks: Vec::new(),
                edges: Vec::new(),
            },
            calls: Vec::new(),
            string_literals: Vec::new(),
            exception_handlers: Vec::new(),
            local_variable_types: Vec::new(),
        };
        let base = class_with_methods("com/example/Base", None, vec![base_method]);
        let derived = class_with_methods(
            "com/example/Derived",
            Some("com/example/Base"),
            vec![override_method],
        );
        let context = context_for(vec![base, derived]);

        let results = NullnessRule.run(&context).expect("nullness rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("parameter 0 is @NonNull but overrides @Nullable"));
    }

    #[test]
    fn nullness_flow_reports_returning_null() {
        let method = method_with(
            "value",
            "()Ljava/lang/String;",
            MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            MethodNullness {
                return_nullness: Nullness::NonNull,
                parameter_nullness: Vec::new(),
            },
            vec![opcodes::ACONST_NULL, opcodes::ARETURN],
            vec![
                Instruction {
                    offset: 0,
                    opcode: opcodes::ACONST_NULL,
                    kind: InstructionKind::Other(opcodes::ACONST_NULL),
                },
                Instruction {
                    offset: 1,
                    opcode: opcodes::ARETURN,
                    kind: InstructionKind::Other(opcodes::ARETURN),
                },
            ],
            Vec::new(),
        );
        let class = class_with_methods("com/example/ReturnNull", None, vec![method]);
        let context = context_for(vec![class]);

        let results = NullnessRule.run(&context).expect("nullness rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("returns null but is @NonNull"));
    }

    #[test]
    fn nullness_flow_reports_nullable_receiver() {
        let method = method_with(
            "invoke",
            "(Ljava/lang/Object;)V",
            MethodAccess {
                is_public: true,
                is_static: true,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            MethodNullness {
                return_nullness: Nullness::Unknown,
                parameter_nullness: vec![Nullness::Nullable],
            },
            vec![
                opcodes::ALOAD_0,
                opcodes::INVOKEVIRTUAL,
                0x00,
                0x01,
                opcodes::RETURN,
            ],
            vec![
                Instruction {
                    offset: 0,
                    opcode: opcodes::ALOAD_0,
                    kind: InstructionKind::Other(opcodes::ALOAD_0),
                },
                Instruction {
                    offset: 1,
                    opcode: opcodes::INVOKEVIRTUAL,
                    kind: InstructionKind::Other(opcodes::INVOKEVIRTUAL),
                },
                Instruction {
                    offset: 4,
                    opcode: opcodes::RETURN,
                    kind: InstructionKind::Other(opcodes::RETURN),
                },
            ],
            vec![CallSite {
                owner: "com/example/Target".to_string(),
                name: "run".to_string(),
                descriptor: "()V".to_string(),
                kind: CallKind::Virtual,
                offset: 1,
            }],
        );
        let class = class_with_methods("com/example/Caller", None, vec![method]);
        let context = context_for(vec![class]);

        let results = NullnessRule.run(&context).expect("nullness rule run");

        assert_eq!(1, results.len());
        let message = results[0].message.text.as_deref().unwrap_or("");
        assert!(message.contains("possible null receiver"));
    }

    #[test]
    fn lookup_return_value_specializes_type_variable_from_receiver_type_argument() {
        let mut callee_method = method_with(
            "methodOne",
            "()Ljava/lang/Object;",
            MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            MethodNullness {
                return_nullness: Nullness::NonNull,
                parameter_nullness: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        callee_method.type_use = Some(MethodTypeUse {
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            return_type: Some(TypeUse {
                nullness: Nullness::NonNull,
                kind: TypeUseKind::TypeVar("T".to_string()),
            }),
        });
        let mut callee_class = class_with_methods("com/example/ClassB", None, vec![callee_method]);
        callee_class.type_parameters = vec![TypeParameterUse {
            name: "T".to_string(),
            class_bound: None,
            interface_bounds: Vec::new(),
        }];
        let mut class_map = BTreeMap::new();
        class_map.insert(callee_class.name.clone(), &callee_class);
        let call = CallSite {
            owner: "com/example/ClassB".to_string(),
            name: "methodOne".to_string(),
            descriptor: "()Ljava/lang/Object;".to_string(),
            kind: CallKind::Virtual,
            offset: 0,
        };
        let target = resolve_call_target(&class_map, &call);
        let receiver = StackValue {
            nullness: Nullness::NonNull,
            type_use: Some(TypeUse {
                nullness: Nullness::NonNull,
                kind: TypeUseKind::Class(ClassTypeUse {
                    name: "com/example/ClassB".to_string(),
                    type_arguments: vec![TypeUse {
                        nullness: Nullness::Nullable,
                        kind: TypeUseKind::Class(ClassTypeUse {
                            name: "java/lang/String".to_string(),
                            type_arguments: Vec::new(),
                            inner: None,
                        }),
                    }],
                    inner: None,
                }),
            }),
            local: None,
            is_null_literal: false,
        };

        let (return_nullness, return_type_use) =
            lookup_return_value(target.as_ref(), call.kind, Some(&receiver));

        assert_eq!(Nullness::Nullable, return_nullness);
        assert_eq!(
            Nullness::Nullable,
            return_type_use
                .as_ref()
                .expect("specialized return type-use")
                .nullness
        );
    }

    #[test]
    fn lookup_return_value_uses_unknown_when_type_variable_is_unresolved() {
        let mut callee_method = method_with(
            "methodOne",
            "()Ljava/lang/Object;",
            MethodAccess {
                is_public: true,
                is_static: false,
                is_abstract: false,
                is_synthetic: false,
                is_bridge: false,
            },
            MethodNullness {
                return_nullness: Nullness::NonNull,
                parameter_nullness: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        callee_method.type_use = Some(MethodTypeUse {
            type_parameters: Vec::new(),
            parameters: Vec::new(),
            return_type: Some(TypeUse {
                nullness: Nullness::NonNull,
                kind: TypeUseKind::TypeVar("T".to_string()),
            }),
        });
        let mut callee_class = class_with_methods("com/example/ClassB", None, vec![callee_method]);
        callee_class.type_parameters = vec![TypeParameterUse {
            name: "T".to_string(),
            class_bound: None,
            interface_bounds: Vec::new(),
        }];
        let mut class_map = BTreeMap::new();
        class_map.insert(callee_class.name.clone(), &callee_class);
        let call = CallSite {
            owner: "com/example/ClassB".to_string(),
            name: "methodOne".to_string(),
            descriptor: "()Ljava/lang/Object;".to_string(),
            kind: CallKind::Virtual,
            offset: 0,
        };
        let target = resolve_call_target(&class_map, &call);

        let (return_nullness, return_type_use) =
            lookup_return_value(target.as_ref(), call.kind, None);

        assert_eq!(Nullness::Unknown, return_nullness);
        assert!(matches!(
            return_type_use.as_ref().map(|type_use| &type_use.kind),
            Some(TypeUseKind::TypeVar(name)) if name == "T"
        ));
    }

    #[test]
    fn nullness_rule_reports_nonnull_return_from_marked_class() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
@NullMarked
public class ClassA {
    public String methodOne() {
        return null;
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("returns null but is @NonNull"))
        );
    }

    #[test]
    fn nullness_flow_allows_receiver_after_non_null_check() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
import org.jspecify.annotations.Nullable;
@NullMarked
class ClassB {
    void methodOne(@Nullable ClassB varOne) {}
}
@NullMarked
public class ClassA {
    public void methodOne(@Nullable ClassB varOne, @Nullable ClassB varTwo) {
        if (varOne != null) {
            varOne.methodOne(varTwo);
        }
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(messages.is_empty(), "messages: {messages:?}");
    }

    #[test]
    fn nullness_rule_skips_unmarked_class_returning_null() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullUnmarked;
@NullUnmarked
public class ClassA {
    public String methodOne() {
        return null;
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let has_nullness = output
            .results
            .iter()
            .any(|result| result.rule_id.as_deref() == Some("NULLNESS"));
        assert!(!has_nullness);
    }

    #[test]
    fn nullness_rule_allows_nullable_return_override() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
import org.jspecify.annotations.Nullable;
@NullMarked
public class ClassA {
    public @Nullable String methodOne() {
        return null;
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let has_nullness = output
            .results
            .iter()
            .any(|result| result.rule_id.as_deref() == Some("NULLNESS"));
        assert!(!has_nullness);
    }

    #[test]
    fn nullness_rule_reports_explicit_nonnull_return_in_unmarked_class() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.NullUnmarked;
@NullUnmarked
public class ClassA {
    public @NonNull String methodOne() {
        return null;
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("returns null but is @NonNull"))
        );
    }

    #[test]
    fn nullness_rule_reports_nullable_parameter_receiver() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.Nullable;
public class ClassA {
    public static void methodOne(@Nullable String varOne) {
        varOne.toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("possible null receiver"))
        );
    }

    #[test]
    fn nullness_rule_allows_unspecified_parameter() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullnessUnspecified;
public class ClassA {
    public static void methodOne(@NullnessUnspecified String varOne) {
        varOne.toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let has_nullness = output
            .results
            .iter()
            .any(|result| result.rule_id.as_deref() == Some("NULLNESS"));
        assert!(!has_nullness);
    }

    #[test]
    fn nullness_rule_reports_override_mismatch_from_annotations() {
        let mut sources = jspecify_stubs();
        sources.extend(vec![
            SourceFile {
                path: "com/example/Base.java".to_string(),
                contents: r#"
package com.example;
import org.jspecify.annotations.NonNull;
public class Base {
    public @NonNull String value() {
        return "ok";
    }
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/Derived.java".to_string(),
                contents: r#"
package com.example;
import org.jspecify.annotations.Nullable;
public class Derived extends Base {
    @Override
    public @Nullable String value() {
        return null;
    }
}
"#
                .to_string(),
            },
        ]);

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert_eq!(1, messages.len(), "messages: {messages:?}");
        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("return type-use is more nullable"))
        );
    }

    #[test]
    fn nullness_rule_reports_type_use_return_override() {
        let mut sources = jspecify_stubs();
        sources.extend(vec![
            SourceFile {
                path: "com/example/Base.java".to_string(),
                contents: r#"
package com.example;
import java.util.Collections;
import java.util.List;
import org.jspecify.annotations.NonNull;
public class Base {
    public List<@NonNull String> methodOne() {
        return Collections.emptyList();
    }
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/Derived.java".to_string(),
                contents: r#"
package com.example;
import java.util.Collections;
import java.util.List;
import org.jspecify.annotations.Nullable;
public class Derived extends Base {
    @Override
    public List<@Nullable String> methodOne() {
        return Collections.emptyList();
    }
}
"#
                .to_string(),
            },
        ]);

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("return type-use is more nullable"))
        );
    }

    #[test]
    fn nullness_rule_reports_type_use_parameter_override() {
        let mut sources = jspecify_stubs();
        sources.extend(vec![
            SourceFile {
                path: "com/example/Base.java".to_string(),
                contents: r#"
package com.example;
import java.util.List;
import org.jspecify.annotations.Nullable;
public class Base {
    public void methodOne(List<@Nullable String> varOne) {}
}
"#
                .to_string(),
            },
            SourceFile {
                path: "com/example/Derived.java".to_string(),
                contents: r#"
package com.example;
import java.util.List;
import org.jspecify.annotations.NonNull;
public class Derived extends Base {
    @Override
    public void methodOne(List<@NonNull String> varOne) {}
}
"#
                .to_string(),
            },
        ]);

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(messages.iter().any(|msg| {
            msg.contains("parameter 0 has a more restrictive") && msg.contains("type-use")
        }));
    }

    #[test]
    fn nullness_rule_reports_type_use_flow_from_generic_call() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
import org.jspecify.annotations.Nullable;
@NullMarked
class ClassB<T> {
    private final T varOne;
    ClassB(T varOne) {
        this.varOne = varOne;
    }
    T methodOne() {
        return varOne;
    }
}
@NullMarked
public class ClassA {
    public void methodOne(ClassB<@Nullable String> varOne) {
        varOne.methodOne().toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("possible null receiver")),
            "messages: {messages:?}"
        );
    }

    #[test]
    fn nullness_rule_allows_type_use_flow_from_generic_call_with_nonnull_argument() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.NullMarked;
@NullMarked
class ClassB<T> {
    private final T varOne;
    ClassB(T varOne) {
        this.varOne = varOne;
    }
    T methodOne() {
        return varOne;
    }
}
@NullMarked
public class ClassA {
    public void methodOne(ClassB<@NonNull String> varOne) {
        varOne.methodOne().toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            !messages
                .iter()
                .any(|msg| msg.contains("possible null receiver")),
            "messages: {messages:?}"
        );
    }

    #[test]
    fn nullness_rule_allows_type_use_flow_from_raw_generic_call() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
@NullMarked
class ClassB<T> {
    private final T varOne;
    ClassB(T varOne) {
        this.varOne = varOne;
    }
    T methodOne() {
        return varOne;
    }
}
@NullMarked
public class ClassA {
    @SuppressWarnings({"rawtypes", "unchecked"})
    public void methodOne(ClassB varOne) {
        varOne.methodOne().toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            !messages
                .iter()
                .any(|msg| msg.contains("possible null receiver")),
            "messages: {messages:?}"
        );
    }

    #[test]
    fn nullness_rule_reports_type_use_flow_from_generic_call_after_local_store() {
        let mut sources = jspecify_stubs();
        sources.push(SourceFile {
            path: "com/example/ClassA.java".to_string(),
            contents: r#"
package com.example;
import org.jspecify.annotations.NullMarked;
import org.jspecify.annotations.Nullable;
@NullMarked
class ClassB<T> {
    private final T varOne;
    ClassB(T varOne) {
        this.varOne = varOne;
    }
    T methodOne() {
        return varOne;
    }
}
@NullMarked
public class ClassA {
    public void methodOne(ClassB<@Nullable String> varOne) {
        String tmpValue = varOne.methodOne();
        tmpValue.toString();
    }
}
"#
            .to_string(),
        });

        let output = analyze_with_harness(sources);
        let messages: Vec<String> = output
            .results
            .iter()
            .filter(|result| result.rule_id.as_deref() == Some("NULLNESS"))
            .filter_map(|result| result.message.text.clone())
            .collect();

        assert!(
            messages
                .iter()
                .any(|msg| msg.contains("possible null receiver")),
            "messages: {messages:?}"
        );
    }
}
