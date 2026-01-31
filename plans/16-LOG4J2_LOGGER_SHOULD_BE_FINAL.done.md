# LOG4J2_LOGGER_SHOULD_BE_FINAL

## Goal
Ensure Log4j2 Logger fields are final to prevent reassignment.

## Detection approach
- Scan class fields for type org/apache/logging/log4j/Logger.
- Report if the field is not final.

## Bytecode signals
- Field descriptors with Lorg/apache/logging/log4j/Logger; and access flags.

## Tests
- Report: private Logger logger;
- Allow: private final Logger logger;

## Edge cases
- Static logger fields should still be final.
- Lazy init patterns may intentionally avoid final; decide if allowed.

## Notes
- Use stub Log4j2 Logger type in harness tests.
