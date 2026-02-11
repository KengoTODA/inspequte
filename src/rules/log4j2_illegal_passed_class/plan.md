# LOG4J2_ILLEGAL_PASSED_CLASS

## Goal
Detect illegal class objects passed to Log4j2 LogManager.getLogger(Class) (e.g., using a different class literal).

## Detection approach
- Match LogManager.getLogger(Class) calls.
- Report when the passed class literal does not match the caller class.
- Ignore overloads that do not take a Class argument.

## Bytecode signals
- INVOKESTATIC on org/apache/logging/log4j/LogManager.getLogger(Ljava/lang/Class;).
- Track class literals from LDC/LDC_W and compare to the current class.

## Tests
- Report: LogManager.getLogger(OtherType.class)
- Allow: LogManager.getLogger(ClassA.class) in ClassA
- Allow: getLogger() overloads without Class parameter

## Edge cases
- Inner classes should allow Outer$Inner.class for the matching class name.
- Unknown class literal should not trigger (no false positives).

## Notes
- Use stub Log4j2 LogManager in harness tests.
