# Performance

## Purpose
- Compare NULLNESS-focused analysis performance across tools using `hyperfine`.
- Keep scope to common nullness semantics rather than total rule count.

## Methodology
- Benchmark metric: median runtime (`hyperfine` JSON `median`).
- Parameters: `--warmup 0`, `--min-runs 1`.
- Generated at (UTC): `2026-02-27T23:59:00Z`.

## Datasets
- Library: Guava `33.5.0-jre`.
  - Binary input: Maven Central JAR.
  - Source input: Maven Central source JAR.
- Application: SonarQube `25.6.0.109173`.
  - Binary input: Maven Central `sonar-application` ZIP.
  - Source input: GitHub tag source archive.

## Tooling and Versions
| Tool | Version | Nullness scope used in this page |
| --- | --- | --- |
| inspequte | workspace build | `NULLNESS` rule only |
| SpotBugs | 4.9.8 | `NP_*` via include filter (`Bug code=NP`) |
| PMD | 7.14.0 | null-related subset in `category/java/errorprone.xml` (threads=1, cache=off) |
| Checker Framework | 3.52.0 | `NullnessChecker` |
| NullAway | 0.12.12 | Error Prone plugin (`error_prone_core 2.30.0`) |

Environment:
- OS: `Darwin`
- Kernel: `25.3.0`
- CPU: `arm64`
- Java: `openjdk version "21.0.10" 2026-01-20 LTS`

## Results: Guava
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| inspequte | 0.250 s | 0.448 s | 0.247 s | 0.847 s |
| nullaway | 1.234 s | 1.234 s | 1.134 s | 1.334 s |
| checker-framework | 1.314 s | 1.314 s | 1.269 s | 1.359 s |
| pmd | 3.785 s | 3.785 s | 3.785 s | 3.785 s |
| spotbugs | 12.397 s | 12.397 s | 12.397 s | 12.397 s |

## Results: SonarQube
| Tool | Median | Mean | Min | Max |
| --- | ---: | ---: | ---: | ---: |
| pmd | 9.411 s | 9.411 s | 9.411 s | 9.411 s |
| inspequte | 13.123 s | 13.123 s | 13.123 s | 13.123 s |
| spotbugs | 466.245 s | 466.245 s | 466.245 s | 466.245 s |

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
