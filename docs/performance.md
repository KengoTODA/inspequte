# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 1`, `--min-runs 5`.
- Generated at (UTC): `2026-11-21T03:49:31Z`.

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
- Kernel: `7.0.0-1020-azure`
- CPU: `AMD EPYC 9V45 96-Core Processor`
- Java: `openjdk version "21.0.13" 2026-10-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.276 s | 0.285 s | 0.274 s | 0.350 s |
| nullaway | 1.426 s | 1.437 s | 1.407 s | 1.471 s |
| checker-framework | 1.769 s | 1.781 s | 1.741 s | 1.863 s |
| pmd | 4.216 s | 4.211 s | 4.158 s | 4.287 s |
| spotbugs | 19.407 s | 19.477 s | 18.666 s | 20.740 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 9.370 s | 9.305 s | 9.084 s | 9.446 s |
| inspequte | 15.372 s | 15.429 s | 15.361 s | 15.584 s |
| spotbugs | 627.874 s | 633.907 s | 602.962 s | 680.358 s |

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
