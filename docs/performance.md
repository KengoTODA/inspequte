# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-09-12T14:19:57Z`.

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
- CPU: `AMD EPYC 7763 64-Core Processor`
- Java: `openjdk version "21.0.12.1" 2026-08-18 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.363 s | 0.364 s | 0.360 s | 0.370 s |
| nullaway | 1.533 s | 1.530 s | 1.514 s | 1.550 s |
| checker-framework | 2.418 s | 2.403 s | 2.285 s | 2.486 s |
| pmd | 6.307 s | 6.280 s | 6.168 s | 6.383 s |
| spotbugs | 28.392 s | 28.132 s | 24.859 s | 30.048 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 16.325 s | 16.348 s | 16.165 s | 16.711 s |
| inspequte | 17.184 s | 17.195 s | 17.142 s | 17.278 s |
| spotbugs | 919.741 s | 916.638 s | 886.999 s | 959.992 s |

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
