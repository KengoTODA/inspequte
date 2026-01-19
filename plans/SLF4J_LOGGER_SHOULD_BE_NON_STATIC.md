# SLF4J_LOGGER_SHOULD_BE_NON_STATIC

## Goal
Ensure Logger fields are instance fields (non-static).

## Detection approach
- Scan class fields for type org/slf4j/Logger.
- Report if the field is static.

## Bytecode signals
- Field descriptors with Lorg/slf4j/Logger; and access flags.

## Tests
- Report: private static final Logger logger;
- Allow: private final Logger logger;

## Edge cases
- Some codebases prefer static loggers; confirm desired policy.
