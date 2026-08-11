# SARIF Compatibility

inspequte emits SARIF 2.1.0 only. The normative format is OASIS SARIF 2.1.0 Plus Errata 01.

## Normative schema

The repository vendors the official schema without modification:

- Specification: OASIS SARIF 2.1.0 Plus Errata 01.
- Source: <https://docs.oasis-open.org/sarif/sarif/v2.1.0/errata01/os/schemas/sarif-schema-2.1.0.json>
- Vendored file: `src/assets/sarif-2.1.0.json`.
- SHA-256: `c3b4bb2d6093897483348925aaa73af03b3e3f4bd4ca38cef26dcb4212a2682e`.
- Provenance: a normative standards artifact published by OASIS and retained byte-for-byte.

The schema is embedded in the CLI for optional runtime validation and is always exercised by the SARIF test suite.

## Checking and refreshing the schema

The offline check verifies that the vendored file has the pinned official checksum:

```bash
scripts/update-sarif-schema.sh --check
```

The remote check downloads the immutable Errata 01 URL, verifies its checksum, and compares it byte-for-byte with the vendored file:

```bash
scripts/update-sarif-schema.sh --refresh
```

To restore the vendored copy from the official URL:

```bash
scripts/update-sarif-schema.sh --update
```

If OASIS publishes another erratum, review the specification and consumer compatibility first. Then update the URL and checksum in the script and this document in the same change. Do not accept an unexplained checksum change.

## GitHub Code Scanning compatibility

GitHub Code Scanning supports a subset of SARIF 2.1.0. The unit and integration tests validate inspequte output against the full official schema, while the main-branch CI uploads an inspequte-generated compatibility report through `github/codeql-action/upload-sarif` as an ingestion canary.

Consumer-specific behavior must not weaken OASIS schema validation. When GitHub ignores an otherwise valid field, retain standards-conforming output and document any presentation limitation.

## Future SARIF versions

SARIF 2.2 is not a production output target. It may be monitored, but inspequte must not emit a newer version until it is finalized, the Rust serialization layer supports it, and required consumers such as GitHub Code Scanning can ingest it.
