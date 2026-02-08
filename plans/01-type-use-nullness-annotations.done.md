# Plan: Type-Use Nullness Annotations (JSpecify)

## Objective
Extend the nullness rule to support type-use annotations like `List<@Nullable Object>`.

## Background
Currently, the nullness rule supports declaration annotations (e.g., `@Nullable String`), but does not handle type-use annotations which are crucial for generic types and more precise nullness tracking.

## Specification
Follow the JSpecify specification: https://jspecify.dev/docs/spec/

### Key Features to Support
1. **Type-use annotations on generic type parameters**
   - `List<@Nullable String>` - list containing nullable strings
   - `List<@NonNull String>` - list containing non-null strings
   - `@Nullable List<String>` - nullable list of strings

2. **Array element nullness**
   - `@Nullable String[]` - nullable array
   - `String @Nullable []` - array of nullable strings

3. **Nested generics**
   - `Map<String, @Nullable Object>`
   - `List<List<@Nullable String>>`

4. **Type bounds with nullness**
   - `<T extends @Nullable Object>`
   - `<T super @NonNull String>`

## Implementation Steps
1. Update the bytecode parser to extract type annotations from:
   - Method signatures (RuntimeVisibleTypeAnnotations attribute)
   - Field signatures
   - Local variable tables

2. Enhance the IR (Intermediate Representation) to track:
   - Type-use nullness information alongside type parameters
   - Distinguish between declaration and type-use annotations

3. Extend the nullness analysis to:
   - Propagate type-use nullness through generic method calls
   - Check assignments involving parameterized types
   - Validate method overrides with generic signatures

4. Add comprehensive test cases:
   - Generic collections with nullable/non-null elements
   - Nested generic types
   - Type bounds and wildcards
   - Array element nullness

## Technical Considerations
- JSpecify treats unannotated types as nullable by default in some contexts
- Need to handle both `@Nullable` and `@NonNull` annotations
- Support for different annotation targets (TYPE_USE vs TYPE_PARAMETER)
- Compatibility with existing nullness checks

## Success Criteria
- Parse and represent type-use annotations in the IR
- Detect violations when nullable values are assigned to non-null type parameters
- Detect violations when non-null type parameters receive nullable values
- Pass all JSpecify specification test cases for type-use annotations
- Maintain backward compatibility with existing nullness checks

## Dependencies
- JSpecify specification document
- cafebabe library capabilities for parsing type annotations
- Current nullness rule implementation

## Estimated Complexity
**High** - Requires significant changes to type system representation and analysis logic.

## Post-mortem
- Added type-use signature parsing and annotation application for methods, fields, and locals.
- Extended nullness override checks to compare nested type-use nullness in generics.
- Follow-up: expand flow analysis to propagate type-use nullness through method calls.
