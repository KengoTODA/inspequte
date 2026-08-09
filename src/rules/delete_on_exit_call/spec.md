---
type: 'Static Analysis Rule'
title: 'File.deleteOnExit call'
description: 'File.deleteOnExit can accumulate pending deletions in long-lived processes'
tags: ['jvm', 'static-analysis']
status: 'stable'
rule_id: 'DELETE_ON_EXIT_CALL'
---

# DELETE_ON_EXIT_CALL

## Summary
- Rule ID: `DELETE_ON_EXIT_CALL`
- Name: File.deleteOnExit call
- Problem: `File.deleteOnExit()` can accumulate pending deletions and create memory/shutdown overhead in long-lived processes.

## What This Rule Reports
This rule reports direct calls to:
- `java/io/File.deleteOnExit()V`

### Examples (reported)
```java
package com.example;
import java.io.File;
public class ClassA {
    public void methodX(File varOne) {
        varOne.deleteOnExit();
    }
}
```

## What This Rule Does Not Report
- Other file APIs such as `File.delete()`.
- Calls that appear only in classpath/dependency classes outside the analysis target.

### Examples (not reported)
```java
package com.example;
import java.io.File;
public class ClassB {
    public boolean methodY(File varOne) {
        return varOne.delete();
    }
}
```

## Recommended Fix
Prefer explicit deletion and explicit error handling near the point where the temporary file is no longer needed.

## Message Shape
Findings are reported as `Avoid File.deleteOnExit() in <class>.<method><descriptor>; prefer explicit deletion with error handling.`
