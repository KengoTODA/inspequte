# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-10-11T08:26:51Z`.

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
- Kernel: `6.17.0-1022-azure`
- CPU: `AMD EPYC 7763 64-Core Processor`
- Java: `openjdk version "21.0.12.1" 2026-08-18 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.372 s | 0.373 s | 0.368 s | 0.382 s |
| nullaway | 1.592 s | 1.593 s | 1.560 s | 1.628 s |
| checker-framework | 2.449 s | 2.462 s | 2.435 s | 2.506 s |
| pmd | 6.635 s | 6.639 s | 6.520 s | 6.718 s |
| spotbugs | 29.346 s | 29.228 s | 27.615 s | 30.493 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 17.408 s | 17.412 s | 17.154 s | 17.667 s |
| inspequte | 18.456 s | 18.448 s | 18.188 s | 18.736 s |
| spotbugs | 984.985 s | 992.693 s | 950.835 s | 1036.751 s |

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
