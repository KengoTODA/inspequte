# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-09-05T03:35:14Z`.

## Datasets
- Library: Guava `33.7.1-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.8.0.126808`.
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
- CPU: `AMD EPYC 9V74 80-Core Processor`
- Java: `openjdk version "21.0.12.1" 2026-08-18 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.398 s | 0.400 s | 0.394 s | 0.408 s |
| nullaway | 1.469 s | 1.469 s | 1.456 s | 1.484 s |
| checker-framework | 2.311 s | 2.301 s | 2.211 s | 2.342 s |
| pmd | 5.834 s | 5.818 s | 5.615 s | 6.005 s |
| spotbugs | 25.858 s | 25.188 s | 22.715 s | 28.143 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 15.028 s | 15.100 s | 14.784 s | 15.777 s |
| inspequte | 18.925 s | 18.907 s | 18.774 s | 18.998 s |
| spotbugs | 941.011 s | 937.527 s | 919.583 s | 958.265 s |

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
