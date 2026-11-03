# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-03T11:26:27Z`.

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
- CPU: `AMD EPYC 9V74 80-Core Processor`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.313 s | 0.312 s | 0.309 s | 0.316 s |
| nullaway | 1.521 s | 1.550 s | 1.496 s | 1.622 s |
| checker-framework | 2.055 s | 2.064 s | 2.034 s | 2.099 s |
| pmd | 4.733 s | 4.770 s | 4.648 s | 5.029 s |
| spotbugs | 21.544 s | 21.505 s | 19.643 s | 23.034 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 13.208 s | 13.121 s | 12.639 s | 13.422 s |
| inspequte | 15.762 s | 15.981 s | 15.432 s | 17.415 s |
| spotbugs | 838.670 s | 834.486 s | 799.379 s | 855.629 s |

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
