# SLF4J_MANUALLY_PROVIDED_MESSAGE

## Goal
Detect patterns where log messages are manually constructed instead of using placeholders.

## Detection approach
- Identify StringBuilder/StringBuffer concatenation or String.format used before Logger calls.
- Report when a computed string is passed to a Logger format overload.

## Bytecode signals
- INVOKEVIRTUAL java/lang/StringBuilder.append and toString before Logger call.
- INVOKESTATIC java/lang/String.format before Logger call.

## Tests
- Report: logger.info("value=" + value)
- Report: logger.info(String.format("value=%s", value))
- Allow: logger.info("value={}", value)
- Allow: logger.info(message) when using message-only overload

## Edge cases
- Avoid flagging when calling message-only overloads.
- Marker overloads should still be checked.
