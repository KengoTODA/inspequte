# Java Class-File Compatibility

Java 21 remains the primary development and test-harness JDK for inspequte. A separate CI lane verifies that the released analyzer can ingest class files produced by the latest explicitly supported Java release.

## Current policy

- Primary test harness: Java 21.
- Latest compatibility lane: Java 26.
- Expected Java 26 class-file major version: 70.
- Compatibility inputs: a compiled class directory and the equivalent JAR.
- Output contract: schema-valid, deterministic SARIF 2.1.0.

The Java 26 lane compiles `tests/fixtures/java-compatibility/ClassA.java` with a Java 26 compiler. The fixture covers records, sealed types, nested classes, generics, lambdas, method references, and debug metadata while using generic harness names.

The lane asserts the class-file header directly before invoking inspequte. This distinguishes a compiler/setup failure from a parser failure. inspequte then scans the directory and JAR twice; timing properties are removed before comparing the reports because they are intentionally volatile.

## Parser behavior

The current `jclassfile` parser reads the class-file major version without imposing a maximum. inspequte also has a deterministic minimal-parser fallback for unknown, non-critical attributes. A major-version-70 regression test exercises that fallback independently from the compiler fixture.

If a future class file introduces a constant-pool entry, bytecode, or required attribute that the external parser cannot decode, the compatibility lane must fail as a parser incompatibility. Do not silently claim support or add a complex compatibility shim. Prefer the smallest supported parser update; otherwise keep the newer Java version unsupported until the dependency is ready.

## Adding a future Java lane

Add or advance a lane only when all of the following are true:

1. The Java release is generally available from the configured JDK distribution.
2. Its class-file major version is final in the JVMS.
3. The tracked fixture compiles without preview flags.
4. Both directory and JAR scans pass schema validation and determinism checks.
5. The parser either supports the format directly or has a small, documented dependency update.

Draft or preview metadata, including evolving null-restricted or nullable type encodings, is monitored but does not imply annotation or rule semantics until the relevant specification is final and explicitly supported.

Run a lane locally with the matching JDK in `JAVA_HOME`:

```bash
cargo build --release
scripts/test-java-compatibility.sh 26 70
```

Java 26 uses class-file major version 70 according to the [Java SE 26 JVM Specification](https://docs.oracle.com/javase/specs/jvms/se26/html/jvms-4.html). The CI JDK is installed through [actions/setup-java](https://github.com/actions/setup-java).
