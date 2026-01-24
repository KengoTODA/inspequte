# Plan: File-Based Input for Long Command-Line Options

## Objective
Add support for reading `--input` and `--classpath` values from local files to handle projects with many dependencies, particularly useful for Gradle projects.

## Motivation
Gradle projects often have extensive dependency lists that result in very long command-line arguments, which can:
- Exceed shell command-line length limits (e.g., 131,072 characters on Linux)
- Be difficult to read and debug
- Cause issues in CI/CD scripts

## Proposed Solutions

### Option 1: Dedicated File-Based Options
Add new command-line options:
```bash
inspequte --input-file inputs.txt --classpath-file classpath.txt --output results.sarif
```

Where `inputs.txt` and `classpath.txt` contain one path per line:
```
# inputs.txt
build/classes/java/main
target/classes

# classpath.txt
lib/commons-lang3-3.12.0.jar
lib/guava-31.1-jre.jar
/home/user/.gradle/caches/modules-2/files-2.1/org.slf4j/slf4j-api/2.0.0/...
```

### Option 2: @ Prefix Convention (Similar to javac)
Allow `@` prefix to reference a file:
```bash
inspequte --input @inputs.txt --classpath @classpath.txt --output results.sarif
```

This follows the convention used by `javac` and other Java tools.

### Option 3: Hybrid Approach
Support both direct values and file references:
```bash
inspequte --input app.jar --input @more-inputs.txt --classpath @classpath.txt --output results.sarif
```

**Recommended**: Option 2 (@ prefix) with support for hybrid usage (Option 3)

## Implementation Details

### File Format
- One path per line
- Support for comments (lines starting with `#`)
- Support for empty lines (ignored)
- Relative paths resolved from the directory containing the file
- Support for both absolute and relative paths

Example file:
```
# Main application classes
build/classes/java/main

# Test classes
build/classes/java/test

# Empty line below is ignored

# Dependencies
lib/*.jar
```

### Path Resolution
1. If path starts with `/`, treat as absolute
2. Otherwise, resolve relative to:
   - Directory containing the reference file (for nested references)
   - Current working directory (for top-level references)

### Glob Pattern Support
Consider supporting glob patterns in files:
```
lib/*.jar
build/**/*.class
```

### Argument Parsing Changes
Modify `src/cfg.rs` (or CLI argument parsing code):
```rust
fn expand_file_reference(arg: &str) -> Result<Vec<String>> {
    if arg.starts_with('@') {
        let file_path = &arg[1..];
        read_paths_from_file(file_path)
    } else {
        Ok(vec![arg.to_string()])
    }
}

fn read_paths_from_file(path: &str) -> Result<Vec<String>> {
    let content = std::fs::read_to_string(path)?;
    let paths: Vec<String> = content
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.to_string())
        .collect();
    Ok(paths)
}
```

## Implementation Steps
1. Add file expansion logic to argument parser
2. Update `--input` and `--classpath` handling to support `@` prefix
3. Implement file reading with:
   - Comment support
   - Empty line handling
   - Error handling for missing files
   - Path resolution logic
4. Update help text and documentation
5. Add tests:
   - Single file reference
   - Multiple file references
   - Mixed direct and file references
   - Nested file references (file referencing another file)
   - Missing file error handling
   - Malformed file handling
6. Update README.md with examples
7. Add integration test with realistic Gradle-like dependency list

## Command-Line Help Updates
```
OPTIONS:
    --input <PATH>...
            Input class files, JAR files, or directories to analyze.
            Can be specified multiple times.
            Use @file.txt to read paths from a file (one per line).

    --classpath <PATH>...
            Classpath for analysis (dependencies, libraries).
            Can be specified multiple times.
            Use @file.txt to read paths from a file (one per line).
```

## Example Usage

### Basic usage with file
```bash
inspequte --input @inputs.txt --classpath @classpath.txt --output results.sarif
```

### Gradle integration
Generate classpath file:
```gradle
task writeClasspath {
    doLast {
        file('build/classpath.txt').text = configurations.runtimeClasspath.join('\n')
        file('build/inputs.txt').text = "build/classes/java/main"
    }
}

task inspequte(type: Exec, dependsOn: [writeClasspath, classes]) {
    commandLine 'inspequte',
        '--input', '@build/inputs.txt',
        '--classpath', '@build/classpath.txt',
        '--output', 'build/inspequte.sarif'
}
```

## Testing Strategy
1. Unit tests for file parsing logic
2. Integration tests with:
   - Small file with 2-3 paths
   - Large file with 100+ paths
   - File with comments and empty lines
   - Mixed direct and file references
3. Error handling tests:
   - Non-existent file
   - Permission denied
   - Malformed paths

## Success Criteria
- Successfully parse paths from files with @ prefix
- Support comments and empty lines in path files
- Proper error messages for missing or malformed files
- Documentation updated with examples
- Tests covering all scenarios
- Works with real-world Gradle projects

## Estimated Complexity
**Low-Medium** - Straightforward feature addition with clear requirements.
