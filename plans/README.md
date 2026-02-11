# Plans Directory

This directory contains detailed implementation plans for future features and improvements to inspequte.

## Purpose

Each plan file documents:
- **Objective**: What we want to achieve
- **Background**: Context and motivation
- **Implementation approach**: Technical details and strategy
- **Test cases**: Expected behavior and edge cases
- **Success criteria**: How to verify completion
- **Dependencies**: Required resources and prerequisites
- **Complexity estimate**: Effort level assessment

## Current Plans

1. **[01-type-use-nullness-annotations.done.md](01-type-use-nullness-annotations.done.md)**
    - Extend nullness rule to support type-use annotations like `List<@Nullable Object>`
    - Based on JSpecify specification
    - Complexity: **High**

2. **[02-java-stdlib-nullness-database.md](02-java-stdlib-nullness-database.md)**
   - Handle nullness of Java standard library APIs
   - Use Checker Framework's nullness database (MIT License)
   - Complexity: **Medium-High**

3. **[03-file-based-classpath-input.md](03-file-based-classpath-input.md)**
   - Accept `--input` and `--classpath` values from files using `@file.txt` syntax
   - Useful for Gradle projects with many dependencies
   - Complexity: **Low-Medium**

4. **[04-improve-agent-documentation.md](04-improve-agent-documentation.md)**
   - Update AGENTS.md to instruct AI agents to use meaningless names in test harness code
   - Prevent name collisions with user examples
   - Complexity: **Low**

5. **[05-prefer-enumset-rule.md](05-prefer-enumset-rule.md)**
   - Rule to prefer `EnumSet` over `HashSet`, `ArrayList`, etc. for enum types
   - Performance optimization recommendation
   - Complexity: **Medium**

6. **[interrupted_exception/plan.md](../src/rules/interrupted_exception/plan.md)**
   - Rule to detect improper `InterruptedException` handling
   - Ensure threads restore interrupt status
   - Complexity: **Medium**

7. **[nullness/plan.md](../src/rules/nullness/plan.md)**
   - Propagate generic type-use nullness through method-call flow analysis
   - Unblock ignored nullness flow test for `ClassB<@Nullable String>` call chains
   - Complexity: **Medium-High**

## Plan Status

Plans are tracked individually; plan 01 and the nullness flow plan are complete, and the remaining plans are in **planning** state.
Implementation priority will be determined based on:
- User requests and feedback
- Impact on analysis quality
- Implementation complexity
- Dependencies on other features

## Contributing

When creating a new plan:
1. Use a descriptive filename with a numeric prefix: `NN-feature-name.md`
2. Include all standard sections: Objective, Background, Implementation, Tests, Success Criteria
3. Estimate complexity: Low, Medium, High, or combinations
4. List all dependencies and prerequisites
5. Consider edge cases and false positives

When implementing a plan:
1. **Rename the plan file** with a `.done.md` suffix after the implementation is complete and merged
   - Example: `01-foo.md` → `01.foo.done.md`
2. This marks completed work while preserving the implementation history

## License Considerations

Some plans involve third-party resources:
- Plan 02 uses Checker Framework stubs (MIT License - compatible with AGPL-3.0)
- Always verify license compatibility before incorporating external data
- Add proper attribution when using third-party resources
