# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-04-30T01:40:08Z`.

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
- CPU: `AMD EPYC 9V74 80-Core Processor`
- Java: `openjdk version "21.0.10" 2026-01-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.319 s | 0.323 s | 0.317 s | 0.345 s |
| nullaway | 1.183 s | 1.189 s | 1.168 s | 1.210 s |
| checker-framework | 1.851 s | 1.840 s | 1.815 s | 1.860 s |
| pmd | 4.614 s | 4.589 s | 4.412 s | 4.736 s |
| spotbugs | 19.674 s | 19.323 s | 17.800 s | 20.189 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 11.888 s | 11.742 s | 11.414 s | 11.926 s |
| inspequte | 17.936 s | 17.982 s | 17.794 s | 18.332 s |
| spotbugs | 821.234 s | 833.665 s | 816.486 s | 857.318 s |

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
