# LOG4J2_ILLEGAL_PASSED_CLASS

## Goal
Detect illegal class objects passed as Log4j2 logger arguments (e.g., using Class as a formatting arg).

## Detection approach
- Match Logger calls and inspect argument types.
- Report if any argument type is java/lang/Class where not allowed by Log4j2 conventions.
- Apply only to String-format overloads (not Message/Supplier overloads).

## Bytecode signals
- Descriptor parameter types include Ljava/lang/Class;.
- For varargs arrays, inspect array element types when known.

## Tests
- Report: logger.info("{}", MyType.class)
- Report: logger.debug(marker, "{}", MyType.class)
- Allow: logger.info("{}", obj)
- Allow: message-only overloads

## Edge cases
- Class passed as marker? Should not match.
- Unknown arg types should not trigger.

## Notes
- Use stub Log4j2 classes in harness tests (Logger, Marker).
