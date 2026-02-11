# LOG4J2_SIGN_ONLY_FORMAT

## Goal
Detect format strings that are only signs or placeholders without meaningful text.

## Detection approach
- Require constant format string.
- Report when the string is empty or composed only of "{}" placeholders and whitespace.

## Bytecode signals
- Constant string from LDC.
- Placeholder counting plus trimmed content checks.

## Tests
- Report: logger.info("{}", arg)
- Report: logger.info("{} {}", a, b)
- Allow: logger.info("value={}", arg)
- Allow: logger.info("{} value", arg)

## Edge cases
- Escaped placeholders should not count.
- Marker overloads and varargs.

## Notes
- Use stub Log4j2 Logger type in harness tests.
