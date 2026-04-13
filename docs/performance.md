# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-04-13T18:58:46Z`.

## Datasets
- Library: Guava `33.5.0-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.4.0.121862`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.9.8 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.13.1 | Error Prone plugin (`error_prone_core 2.49.0`) |

Environment:
- OS: `Linux`
- Kernel: `6.17.0-1010-azure`
- CPU: `AMD EPYC 7763 64-Core Processor`
- Java: `openjdk version "21.0.10" 2026-01-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.364 s | 0.365 s | 0.362 s | 0.371 s |
| nullaway | 1.589 s | 1.592 s | 1.565 s | 1.615 s |
| checker-framework | 2.487 s | 2.481 s | 2.402 s | 2.549 s |
| pmd | 6.291 s | 6.348 s | 6.235 s | 6.528 s |
| spotbugs | 27.338 s | 27.413 s | 26.632 s | 28.756 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 16.544 s | 16.469 s | 16.020 s | 16.767 s |
| inspequte | 20.023 s | 20.059 s | 19.956 s | 20.168 s |
| spotbugs | 1012.548 s | 1003.421 s | 987.560 s | 1015.094 s |

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
