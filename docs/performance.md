# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-22T03:05:04Z`.

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
- Kernel: `7.0.0-1016-aws`
- CPU: `Intel(R) Xeon(R) 6975P-C`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.243 s | 0.243 s | 0.237 s | 0.248 s |
| nullaway | 1.409 s | 1.404 s | 1.380 s | 1.422 s |
| checker-framework | 1.709 s | 1.719 s | 1.699 s | 1.743 s |
| pmd | 4.306 s | 4.330 s | 4.239 s | 4.469 s |
| spotbugs | 19.391 s | 18.907 s | 16.874 s | 20.150 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 11.215 s | 11.164 s | 10.790 s | 11.438 s |
| inspequte | 12.179 s | 12.178 s | 12.137 s | 12.201 s |
| spotbugs | 775.685 s | 778.263 s | 773.217 s | 791.020 s |

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
