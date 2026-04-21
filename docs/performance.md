# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-04-21T13:25:11Z`.

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
| inspequte | 0.365 s | 0.366 s | 0.364 s | 0.374 s |
| nullaway | 1.635 s | 1.637 s | 1.596 s | 1.672 s |
| checker-framework | 2.523 s | 2.512 s | 2.427 s | 2.564 s |
| pmd | 6.568 s | 6.620 s | 6.511 s | 6.871 s |
| spotbugs | 27.389 s | 26.841 s | 24.678 s | 29.330 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 16.658 s | 16.429 s | 15.828 s | 16.690 s |
| inspequte | 20.735 s | 20.725 s | 20.478 s | 20.869 s |
| spotbugs | 1015.688 s | 1012.727 s | 988.710 s | 1029.156 s |

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
