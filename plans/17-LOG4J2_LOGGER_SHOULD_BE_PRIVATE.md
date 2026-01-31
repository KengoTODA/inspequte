# LOG4J2_LOGGER_SHOULD_BE_PRIVATE

## Goal
Ensure Log4j2 Logger fields are private to avoid exposure and misuse.

## Detection approach
- Scan class fields for type org/apache/logging/log4j/Logger.
- Report if visibility is not private.

## Bytecode signals
- Field descriptors with Lorg/apache/logging/log4j/Logger; and access flags.

## Tests
- Report: public Logger logger;
- Report: protected Logger logger;
- Allow: private Logger logger;

## Edge cases
- Nested classes and synthetic fields.
- Fields with logger subclasses (if any) should be considered.

## Notes
- Use stub Log4j2 Logger type in harness tests.
