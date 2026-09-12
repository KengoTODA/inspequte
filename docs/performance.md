# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-09-12T19:42:42Z`.

## Datasets
- Library: Guava `33.7.1-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.9.0.129388`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.10.4 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.14.1 | Error Prone plugin (`error_prone_core 2.50.0`) |

Environment:
- OS: `Linux`
- Kernel: `6.17.0-1022-azure`
- CPU: `INTEL(R) XEON(R) PLATINUM 8573C`
- Java: `openjdk version "21.0.12.1" 2026-08-18 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.344 s | 0.343 s | 0.332 s | 0.355 s |
| nullaway | 1.427 s | 1.424 s | 1.380 s | 1.446 s |
| checker-framework | 2.199 s | 2.208 s | 2.117 s | 2.309 s |
| pmd | 5.519 s | 5.479 s | 5.359 s | 5.580 s |
| spotbugs | 22.177 s | 23.294 s | 21.938 s | 25.145 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 13.829 s | 13.934 s | 13.633 s | 14.479 s |
| inspequte | 15.930 s | 16.033 s | 15.594 s | 16.650 s |
| spotbugs | 912.622 s | 913.736 s | 906.110 s | 924.999 s |

## Caveats and Fairness
- Rule sets are aligned to nullness intent, not full one-to-one semantic equivalence.
- Source-oriented tools and bytecode-oriented tools have different execution models.
- For operational comparison, an order-of-magnitude speed difference is acceptable in this report.
- This page compares performance only, not detection quality or precision/recall.

## Repro Command
```bash
bash scripts/bench-nullness-compare.sh --dataset all --min-runs 5 --warmup 1
bash scripts/render-performance-docs.sh
```
