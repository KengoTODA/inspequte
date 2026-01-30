# LOG4J2_FORMAT_SHOULD_BE_CONST

## Goal
Detect Log4j2 logging calls where the format string is not a compile-time constant.

## Detection approach
- Match calls to org/apache/logging/log4j/Logger logging methods.
- Identify overloads with (String, ...), (Marker, String, ...), and varargs forms.
- Flag when the format argument is not a constant string (LDC) and is not a known constant local.
- Ignore Message or Supplier overloads (message-only, no format placeholders).

## Bytecode signals
- Track const strings from LDC/LDC_W.
- Track local stores/loads for simple propagation.
- For INVOKExxx on Logger, resolve descriptor and locate format argument index.

## Tests
- Report: logger.info(new String("x {}"), arg)
- Report: logger.info(prefix + " {}", arg)
- Allow: logger.info("x {}", arg)
- Allow: logger.debug(marker, "x {}", arg)
- Allow: message-only overloads (Message or Supplier)

## Edge cases
- Marker overloads shift format index.
- Varargs overloads should still require constant format.
- Ignore if format cannot be determined (unknown propagation).

## Notes
- Use stub Log4j2 classes in harness tests (Logger, Marker, Message) to avoid external jars.
