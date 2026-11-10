# ADR 0008: Report Unknown or Unsupported rather than guess

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Static complexity analysis of real C++ is undecidable in general and incomplete in practice. When the analyzer meets something it cannot model, it can either guess the most plausible complexity or say it does not know. A wrong "O(n)" with high confidence is worse than no answer, because users act on it.

## Decision

Every result carries a **`BoundStatus`** and a **`Confidence`**, and the analyzer applies these rules conservatively:

| Situation | `bound_status` | Confidence |
|---|---|---|
| Derived from the code under the stated assumptions, with no limitations and no early exit | `proven` | `high` |
| `proven`, but with limitations (such as uncounted library calls) or an early exit (still an upper bound) | `proven` | `medium` |
| A bound variable may be written in the body, or a user-defined function is called (callee cost not included) | `estimated` | `low` |
| Data-dependent termination (for example `while (lo < hi)`, Collatz) | `unknown` | `none` |
| Index written in the body, `do-while`, recursion (no recurrence solver) | `unsupported` | `none` |

Further rules:

- Workloads are `unknown` with `missing_bounds` when a variable lacks a maximum. They are `estimated` when monotonicity cannot be shown. They are `unknown` when evaluation overflows or exceeds its budget.
- Memory figures are lower bounds. The engine never reports a peak.
- `summary.coverage` is always `"partial"`.
- Assumptions and limitations are listed explicitly on every finding and workload.
- `models::weakest()` combines statuses so that no combination is reported stronger than its weakest part.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Best-guess heuristics, such as "while loops are O(log n)" | Confidently wrong on common code (fixture `10_data_dependent_termination.cpp`). |
| Omit findings the analyzer cannot bound | Hides risk. The user would not know a loop was skipped. |
| A single numeric confidence score | Less clear than discrete statuses, and invites false precision. |

## Consequences

**Positive**

- Users can trust `proven` results and can see exactly what is missing.
- Tests can assert honesty: `BinarySearchWhileLoopIsUnknownNotGuessed`, `DataDependentTerminationIsUnknown`, `ModifiedIndexIsUnsupported`, `NaiveFibonacciRecursionIsUnsupported`, `MissingConstraintsLeaveWorkloadUnknown`, `MemoryNeverLikelySafeFromLowerBoundsOnly`.

**Negative**

- Many real programs will produce `unknown` or `unsupported` findings until coverage grows.
- More output for the user to read (assumptions and limitations).

## Follow-up considerations

- Grow coverage only with test fixtures that pin the new status rules.
- Recurrence solving (planned) would move recursion from `unsupported` to `proven` or `estimated` for recognised shapes.
- See [LIMITATIONS](../../specifications/LIMITATIONS.md) and [ANALYSIS_MODEL](../../design/ANALYSIS_MODEL.md).
