# STRING_FORMAT_LOCALE_MISSING

## Summary
- Rule ID: `STRING_FORMAT_LOCALE_MISSING`
- Name: String/Formatter formatting without explicit locale
- Description: Reports `String.format(...)` and `Formatter` usage that omits `Locale`, because output becomes dependent on the runtime default locale.
- Annotation policy: `@Suppress`/`@SuppressWarnings` are not supported; only JSpecify annotations are recognized for annotation-driven semantics, and non-JSpecify annotations do not change behavior.

## Motivation
Formatting APIs that use the default locale can produce different output across environments (for example, number/date formatting differences). Requiring an explicit locale keeps results deterministic.

## What it detects
- Calls to `java.lang.String.format(String, Object...)`.
- Calls to `java.util.Formatter.format(String, Object...)`.
- Constructors of `java.util.Formatter` that do not receive a `Locale`.
- One finding per matching call site.

## What it does NOT detect
- Locale-aware overloads that already pass a `java.util.Locale` argument.
- Other formatting APIs outside `String.format` and `Formatter`.
- Suppression via annotations (`@Suppress`, `@SuppressWarnings`).
- Behavior changes based on non-JSpecify annotations.

## Output
- Message should be actionable and include method context, for example:
  `Formatting in <class>.<method><descriptor> depends on the default locale; pass Locale.ROOT (or another explicit Locale).`
- Location should point to the call site line when line metadata is available.

## Acceptance criteria
- Reports each supported `String.format(...)`/`Formatter` call that omits `Locale`.
- Does not report locale-aware overloads with a `Locale` argument.
- Covers TP and TN scenarios with tests.
- Produces deterministic finding ordering.
