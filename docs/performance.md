# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-05-09T11:11:42Z`.

## Datasets
- Library: Guava `33.6.0-jre`.
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
| NullAway | 0.13.4 | Error Prone plugin (`error_prone_core 2.49.0`) |

Environment:
- OS: `Linux`
- Kernel: `6.17.0-1010-azure`
- CPU: `AMD EPYC 7763 64-Core Processor`
- Java: `openjdk version "21.0.10" 2026-01-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.366 s | 0.367 s | 0.363 s | 0.374 s |
| nullaway | 1.538 s | 1.542 s | 1.526 s | 1.565 s |
| checker-framework | 2.363 s | 2.367 s | 2.332 s | 2.428 s |
| pmd | 6.252 s | 6.280 s | 6.184 s | 6.397 s |
| spotbugs | 25.550 s | 26.202 s | 23.840 s | 28.407 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 16.514 s | 16.479 s | 16.007 s | 16.793 s |
| inspequte | 19.547 s | 19.540 s | 19.479 s | 19.598 s |
| spotbugs | 989.398 s | 992.815 s | 982.586 s | 1014.504 s |

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
