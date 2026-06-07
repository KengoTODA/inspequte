# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-06-07T08:37:15Z`.

## Datasets
- Library: Guava `33.6.0-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.6.0.123539`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.9.8 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.13.4 | Error Prone plugin (`error_prone_core 2.49.0`) |

Environment:
- OS: `Linux`
- Kernel: `6.17.0-1015-azure`
- CPU: `AMD EPYC 9V74 80-Core Processor`
- Java: `openjdk version "21.0.11" 2026-04-21 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.403 s | 0.406 s | 0.401 s | 0.418 s |
| nullaway | 1.546 s | 1.549 s | 1.518 s | 1.579 s |
| checker-framework | 2.350 s | 2.336 s | 2.246 s | 2.378 s |
| pmd | 5.860 s | 5.868 s | 5.799 s | 5.976 s |
| spotbugs | 24.426 s | 24.694 s | 23.099 s | 26.660 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 13.674 s | 13.721 s | 13.313 s | 14.114 s |
| inspequte | 22.381 s | 22.441 s | 22.170 s | 22.900 s |
| spotbugs | 1011.504 s | 1009.917 s | 1001.953 s | 1016.613 s |

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
