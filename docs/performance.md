# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-14T04:26:23Z`.

## Datasets
- Library: Guava `33.7.2-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `26.11.0.134175`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.10.5 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.14.3 | Error Prone plugin (`error_prone_core 2.52.0`) |

Environment:
- OS: `Linux`
- Kernel: `7.0.0-1019-azure`
- CPU: `AMD EPYC 9V74 80-Core Processor`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.305 s | 0.305 s | 0.302 s | 0.309 s |
| nullaway | 1.498 s | 1.492 s | 1.481 s | 1.500 s |
| checker-framework | 1.920 s | 1.957 s | 1.888 s | 2.073 s |
| pmd | 4.768 s | 4.725 s | 4.565 s | 4.793 s |
| spotbugs | 21.965 s | 21.159 s | 19.677 s | 22.208 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 12.083 s | 12.216 s | 12.052 s | 12.613 s |
| inspequte | 15.136 s | 15.265 s | 15.069 s | 15.804 s |
| spotbugs | 790.671 s | 790.257 s | 774.988 s | 801.072 s |

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
