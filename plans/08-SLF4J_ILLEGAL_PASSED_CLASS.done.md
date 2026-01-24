# SLF4J_ILLEGAL_PASSED_CLASS

## Goal
Detect illegal class objects passed as logger arguments (e.g., using Class as a formatting arg).

## Detection approach
- Match Logger calls and inspect argument types.
- Report if any argument type is java/lang/Class where not allowed by SLF4J conventions.

## Bytecode signals
- Descriptor parameter types include Ljava/lang/Class;.
- For varargs arrays, inspect array element types when known.

## Tests
- Report: logger.info("{}", MyType.class)
- Report: logger.debug(marker, "{}", MyType.class)
- Allow: logger.info("{}", obj)

## Edge cases
- Class passed as marker? Should not match.
- Unknown arg types should not trigger.
