# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-03T11:06:29Z`.

## Datasets
- Library: Guava `33.7.2-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.10.0.132816`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.10.4 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.14.2 | Error Prone plugin (`error_prone_core 2.50.0`) |

Environment:
- OS: `Linux`
- Kernel: `7.0.0-1017-azure`
- CPU: `AMD EPYC 9V45 96-Core Processor`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.276 s | 0.277 s | 0.272 s | 0.284 s |
| nullaway | 1.260 s | 1.255 s | 1.212 s | 1.302 s |
| checker-framework | 1.720 s | 1.703 s | 1.647 s | 1.759 s |
| pmd | 3.917 s | 3.935 s | 3.864 s | 4.036 s |
| spotbugs | 18.367 s | 18.070 s | 16.512 s | 18.757 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 10.084 s | 10.097 s | 9.715 s | 10.607 s |
| inspequte | 13.699 s | 13.558 s | 12.863 s | 14.192 s |
| spotbugs | 681.548 s | 677.132 s | 664.049 s | 686.255 s |

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
