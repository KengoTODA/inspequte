# Plans Directory

This directory stores cross-cutting plans for inspequte.

Rule-specific plans are now colocated with each rule under:

```text
src/rules/<rule-id>/plan.md
```

## Purpose

Each plan file should document:
- **Objective**: What we want to achieve
- **Background**: Context and motivation
- **Implementation approach**: Technical details and strategy
- **Test cases**: Expected behavior and edge cases
- **Success criteria**: How to verify completion
- **Dependencies**: Required resources and prerequisites
- **Complexity estimate**: Effort level assessment

## Plans In This Directory

1. **[01-type-use-nullness-annotations.done.md](01-type-use-nullness-annotations.done.md)**
   - Extend nullness rule to support type-use annotations like `List<@Nullable Object>`
   - Complexity: **High**
   - Status: **Done**

2. **[02-java-stdlib-nullness-database.md](02-java-stdlib-nullness-database.md)**
   - Handle nullness of Java standard library APIs
   - Use Checker Framework's nullness database (MIT License)
   - Complexity: **Medium-High**
   - Status: **Planning**

3. **[03-file-based-classpath-input.done.md](03-file-based-classpath-input.done.md)**
   - Accept `--input` and `--classpath` values from files using `@file.txt` syntax
   - Complexity: **Low-Medium**
   - Status: **Done**

4. **[04.improve-agent-documentation.done.md](04.improve-agent-documentation.done.md)**
   - Update AGENTS guidance for test harness naming
   - Complexity: **Low**
   - Status: **Done**

5. **[05-worklist-analysis-engine.done.md](05-worklist-analysis-engine.done.md)**
   - Provide a shared deterministic worklist analysis engine
   - Complexity: **High**
   - Status: **Done**

6. **[06-shared-stack-machine-abstraction.done.md](06-shared-stack-machine-abstraction.done.md)**
   - Share JVM stack-machine analysis infrastructure across rules
   - Complexity: **High**
   - Status: **Done**

7. **[07-table-driven-opcode-semantics.done.md](07-table-driven-opcode-semantics.done.md)**
   - Centralize opcode semantics in deterministic tables
   - Complexity: **Medium-High**
   - Status: **Done**

8. **[08-abstract-domain-traits.md](08-abstract-domain-traits.md)**
   - Define reusable trait-based abstract domains
   - Complexity: **Medium-High**
   - Status: **Planning**

9. **[09-analysis-safety-budgeting.md](09-analysis-safety-budgeting.md)**
   - Standardize analysis budgets and diagnostics
   - Complexity: **Medium**
   - Status: **Planning**

10. **[10-oss-fp-hunting-skill.done.md](10-oss-fp-hunting-skill.done.md)**
    - Reproducibly hunt false positives in pinned OSS fixtures
    - Complexity: **High**
    - Status: **Done**

11. **[11-agent-facing-sarif-remediation-contract.md](11-agent-facing-sarif-remediation-contract.md)**
    - Enrich SARIF rule descriptors into an agent-facing remediation contract
    - Complexity: **Medium-High**
    - Status: **Planning**

12. **[12-stable-finding-identities.md](12-stable-finding-identities.md)**
    - Introduce versioned semantic identities for findings and baselines
    - Complexity: **High**
    - Status: **Planning**

13. **[13-sarif-code-flows-and-related-locations.md](13-sarif-code-flows-and-related-locations.md)**
    - Emit concise witness paths and related locations for multi-step findings
    - Complexity: **High**
    - Status: **Planning**

14. **[14-official-oasis-sarif-schema.done.md](14-official-oasis-sarif-schema.done.md)**
    - Adopt the official OASIS SARIF 2.1.0 Errata 01 schema as normative
    - Complexity: **Medium**
    - Status: **Done**

15. **[15-bounded-rule-authoring-loop.md](15-bounded-rule-authoring-loop.md)**
    - Close rule authoring into a bounded, evidence-driven improvement loop
    - Complexity: **High**
    - Status: **Planning**

16. **[16-java-26-compatibility-lane.done.md](16-java-26-compatibility-lane.done.md)**
    - Add a Java 26 class-file compatibility lane while retaining Java 21
    - Complexity: **Medium**
    - Status: **Done**

## Plan Status

Open cross-cutting work in this directory:
- `02-java-stdlib-nullness-database.md`
- `08-abstract-domain-traits.md`
- `09-analysis-safety-budgeting.md`
- `11-agent-facing-sarif-remediation-contract.md`
- `12-stable-finding-identities.md`
- `13-sarif-code-flows-and-related-locations.md`
- `15-bounded-rule-authoring-loop.md`

Implementation priority is determined by:
- User requests and feedback
- Impact on analysis quality
- Implementation complexity
- Dependencies on other features

## Contributing

When creating a new cross-cutting plan in this directory:
1. Use a descriptive filename with a numeric prefix: `NN-feature-name.md`
2. Include all standard sections: Objective, Background, Implementation, Tests, Success Criteria
3. Estimate complexity: Low, Medium, High, or combinations
4. List all dependencies and prerequisites
5. Consider edge cases and false positives

When implementing a cross-cutting plan in this directory:
1. Rename the plan file with a `.done.md` suffix after implementation is complete and merged
2. Add a short post-mortem section (what went well, what was tricky, follow-ups)

When implementing a rule-specific plan:
1. Keep the file as `src/rules/<rule-id>/plan.md`
2. Add a short post-mortem section in that file when the work is complete

## License Considerations

Some plans involve third-party resources:
- Plan 02 uses Checker Framework stubs (MIT License - compatible with AGPL-3.0)
- Always verify license compatibility before incorporating external data
- Add proper attribution when using third-party resources
