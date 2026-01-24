# SLF4J_FORMAT_SHOULD_BE_CONST

## Goal
Detect SLF4J logging calls where the format string is not a compile-time constant.

## Detection approach
- Match calls to org/slf4j/Logger logging methods.
- Identify overloads with (String, ...) and (Marker, String, ...).
- Flag when the format argument is not a constant string (LDC) and is not a known constant local.

## Bytecode signals
- Track const strings from LDC/LDC_W.
- Track local stores/loads for simple propagation.
- For INVOKExxx on Logger, resolve descriptor and locate format argument index.

## Tests
- Report: logger.info(new String("x {}"), arg)
- Report: logger.info(prefix + " {}", arg)
- Allow: logger.info("x {}", arg)
- Allow: logger.debug(marker, "x {}", arg)
- Allow: message-only overloads without format placeholders

## Edge cases
- Marker overloads shift format index.
- Varargs overloads should still require constant format.
- Ignore if format cannot be determined (unknown propagation).
