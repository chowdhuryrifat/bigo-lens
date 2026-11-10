# ADR 0006: Combine static analysis with dynamic measurement (hybrid analysis)

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Static analysis gives structural counts and bounds, but it cannot honestly predict wall-clock time. Constant factors, cache behaviour, compiler optimisation, I/O and library internals all dominate in practice. Some loops, such as binary search with `while (lo < hi)` or Collatz-style loops, cannot be bounded statically at all. Measurement alone, on the other hand, cannot prove anything: a fitted curve describes only the sizes that were measured.

## Decision

BigO Lens uses **two complementary channels whose results are labelled separately**:

1. **Static analysis** (Partial; M2 and M3): Clang-based structural counts, symbolic expressions, constraint workloads and memory lower bounds, each with a `BoundStatus` and `Confidence`.
2. **Dynamic benchmarking** (Planned M4): compile and run the user's code on generated inputs over a range of n, then summarise with robust statistics and fit growth models. The statistics and fitting already exist in `bigolens_benchmark` (`summarize`, `fitGrowth`). The runner does not.

Rules for combining them:

- Static results may be `proven`. Empirical results never are. A fitted model "describes the measured range only".
- `fitGrowth` selects a model only when the best RSS is less than 0.5 times the runner-up's and the slope is positive. Otherwise the result is ambiguous and the UI must say so.
- Time feasibility needs a runtime estimate (`feasibility::RuntimeEstimate`), which only measurement can provide. Until M4 it is always `Unknown`.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Static only, with operations-per-second heuristics (for example "1e8 operations per second") | Presents a folk constant as a prediction. It contradicts [ADR 0007](0007-uncertainty-aware-runtime-estimates.md). |
| Dynamic only | No proofs, no explanation, needs runnable inputs, and has security implications for every analysis. |
| Merge both into a single number | Hides where each claim came from. |

## Consequences

**Positive**

- Each claim is supported by the right kind of evidence.
- Static analysis stays safe because it never executes code. Execution is opt-in.

**Negative**

- Benchmarking brings significant security and platform work: sandboxing and resource limits ([THREAT_MODEL](../../engineering/THREAT_MODEL.md)).
- Two result channels make the UI more complex.

## Follow-up considerations

- M4: compiler adapter, input generators, runner, resource monitor, platform isolation, and wiring of the `benchmark` action (which currently returns `not_implemented`).
- Cross-checking: flag cases where the empirical growth contradicts the static bound.
- See [BENCHMARK_SPECIFICATION](../../specifications/BENCHMARK_SPECIFICATION.md).
