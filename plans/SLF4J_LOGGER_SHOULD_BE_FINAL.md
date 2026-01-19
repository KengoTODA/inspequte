# SLF4J_LOGGER_SHOULD_BE_FINAL

## Goal
Ensure Logger fields are final to prevent reassignment.

## Detection approach
- Scan class fields for type org/slf4j/Logger.
- Report if the field is not final.

## Bytecode signals
- Field descriptors with Lorg/slf4j/Logger; and access flags.

## Tests
- Report: private Logger logger;
- Allow: private final Logger logger;

## Edge cases
- Static logger fields should still be final.
- Lazy init patterns may intentionally avoid final; decide if allowed.
