# ADR 0007: Express runtime estimates as uncertainty ranges, never as single predictions

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Users want to know whether their solution will pass a time limit. A single number such as "1.3 s", or a verdict based on a fixed operations-per-second rule, suggests a precision that does not exist. Hardware, compiler flags, input distribution and constant factors all vary. Presenting false certainty is the most damaging mistake this tool could make.

## Decision

- A runtime estimate is a **range**: `feasibility::RuntimeEstimate { lowMs, centralMs, highMs }`.
- Time feasibility (`feasibility::assessTime`) is decided against the range under an explicit `FeasibilityPolicy`:
  - `HighRisk` if even the optimistic value (`lowMs`) exceeds the limit.
  - `LikelySafe` only if the pessimistic value (`highMs`) is at most `timeSafeFraction` (0.5) of the limit.
  - `Borderline` otherwise.
  - `Unknown` if there is no limit or no estimate.
- **Structural workload counts are never converted to milliseconds.** No runtime estimate exists without measurement, so **time feasibility is always `Unknown` today**. The `capabilities` response says this with `time_feasibility: requires_runtime_estimate`.
- Every result carries its `reason`. Benchmark statistics use robust measures (median, MAD, `relativeSpread`), and `summarize` rejects invalid samples instead of repairing them.
- The same principle applies to memory. Static analysis yields lower bounds only, so `assessMemory` can report `HighRisk` (a lower bound already exceeds the limit) but never `LikelySafe` without a known upper bound.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Fixed operations-per-second constant | Unfounded, and varies by orders of magnitude with the operation mix. |
| A single point estimate from a fitted curve | Hides measurement noise and extrapolation error. |
| Safe or unsafe binary verdict | No room for "borderline" or "unknown". |

## Consequences

**Positive**

- Users are never told a risky solution is "safe" without evidence.
- The status vocabulary (`likely_safe`, `borderline`, `high_risk`, `unknown`) is honest and stable.

**Negative**

- Until M4, users get workload counts such as 4e10 but no time verdict, and must interpret the counts themselves.
- Choosing policy thresholds is a judgement call. They are explicit fields in `FeasibilityPolicy`.

## Follow-up considerations

- M4: derive `RuntimeEstimate` from benchmark measurements, including how extrapolation beyond the measured n widens the range.
- Consider making the policy fractions user-configurable.
- See [FEASIBILITY_MODEL](../../specifications/FEASIBILITY_MODEL.md).
