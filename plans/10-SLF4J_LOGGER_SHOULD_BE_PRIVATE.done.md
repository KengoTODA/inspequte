# SLF4J_LOGGER_SHOULD_BE_PRIVATE

## Goal
Ensure Logger fields are private to avoid exposure and misuse.

## Detection approach
- Scan class fields for type org/slf4j/Logger.
- Report if visibility is not private.

## Bytecode signals
- Field descriptors with Lorg/slf4j/Logger; and access flags.

## Tests
- Report: public Logger logger;
- Report: protected Logger logger;
- Allow: private Logger logger;

## Edge cases
- Nested classes and synthetic fields.
- Fields with logger subclasses (if any) should be considered.
