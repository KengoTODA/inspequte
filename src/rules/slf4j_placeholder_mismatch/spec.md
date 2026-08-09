---
type: 'Static Analysis Rule'
title: 'SLF4J placeholder mismatch'
description: 'SLF4J placeholder count does not match arguments'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'SLF4J_PLACEHOLDER_MISMATCH'
---

# SLF4J_PLACEHOLDER_MISMATCH

## Summary
- Rule ID: `SLF4J_PLACEHOLDER_MISMATCH`
- Name: SLF4J placeholder mismatch
- Problem: Placeholder count mismatch makes logs confusing and can hide missing context.

## What This Rule Reports
This rule reports SLF4J format calls where placeholder count and supplied arguments do not match.
It handles escaped placeholders and common varargs/marker forms.

### Java Example (reported)
```java
LOG.info("user={} action={}", varOne);
```

## What This Rule Does Not Report
- Correctly matched placeholder/argument counts
- Escaped placeholder text that should not count
- Supported marker/throwable patterns where argument treatment differs

### Java Example (not reported)
```java
LOG.info("user={} action={}", varOne, varTwo);
```

## Recommended Fix
Align placeholder count with provided arguments, or rewrite message/arguments for clarity.

## Message Shape
Findings describe expected vs actual argument count for the SLF4J format string.
