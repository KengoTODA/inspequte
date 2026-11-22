# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-22T07:38:27Z`.

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
| NullAway | 0.14.3 | Error Prone plugin (`error_prone_core 2.53.0`) |

Environment:
- OS: `Linux`
- Kernel: `7.0.0-1020-azure`
- CPU: `AMD EPYC 9V45 96-Core Processor`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.255 s | 0.258 s | 0.247 s | 0.271 s |
| nullaway | 1.201 s | 1.200 s | 1.167 s | 1.227 s |
| checker-framework | 1.563 s | 1.566 s | 1.524 s | 1.603 s |
| pmd | 3.720 s | 3.719 s | 3.632 s | 3.808 s |
| spotbugs | 16.187 s | 16.251 s | 15.670 s | 16.681 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 9.661 s | 9.649 s | 9.455 s | 9.799 s |
| inspequte | 12.895 s | 12.763 s | 12.336 s | 13.034 s |
| spotbugs | 680.691 s | 672.720 s | 629.016 s | 699.776 s |

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
