# LOG4J2_UNKNOWN_ARRAY

## Goal
Detect Log4j2 varargs calls where the argument array is not statically known, making placeholder checks unreliable.

## Detection approach
- Match Logger overloads with Object[] / Object... parameters.
- For calls with varargs arrays, inspect stack value for array length when possible.
- Report when the array length is unknown (e.g., passed-in array or built via non-constant size).

## Bytecode signals
- NEWARRAY/ANEWARRAY with constant size implies known length.
- AASTORE may indicate manual population but length still known if size constant.
- ALOAD of array local without known provenance should be treated as unknown.

## Tests
- Report: logger.info("{} {}", argsArray)
- Report: logger.debug(marker, "{}", argsArray)
- Allow: logger.info("{} {}", new Object[]{a, b})
- Allow: logger.info("{} {}", "a", "b")

## Edge cases
- Marker overloads.
- Throwable last argument should not be counted as a placeholder arg.
- If array length unknown, avoid double-reporting with placeholder mismatch.

## Notes
- Use stub Log4j2 Logger type in harness tests.
