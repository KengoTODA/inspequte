# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-09-12T03:16:39Z`.

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
| inspequte | 0.337 s | 0.336 s | 0.329 s | 0.345 s |
| nullaway | 1.406 s | 1.402 s | 1.378 s | 1.425 s |
| checker-framework | 2.214 s | 2.200 s | 2.147 s | 2.275 s |
| pmd | 5.485 s | 5.476 s | 5.383 s | 5.577 s |
| spotbugs | 24.330 s | 23.622 s | 20.884 s | 24.770 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 13.432 s | 13.410 s | 13.087 s | 13.712 s |
| inspequte | 15.391 s | 15.405 s | 15.327 s | 15.490 s |
| spotbugs | 872.925 s | 873.955 s | 867.986 s | 883.452 s |

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
