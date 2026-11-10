# Analysis Model

This document defines **what BigO Lens computes, what each result means, and what it does not mean**. It is the
conceptual reference; exact algorithms are in the specifications:

- [COMPLEXITY_MODEL.md](../specifications/COMPLEXITY_MODEL.md) — expressions, loop forms, status rules
- [CONSTRAINT_MODEL.md](../specifications/CONSTRAINT_MODEL.md) — constraints and workload evaluation
- [FEASIBILITY_MODEL.md](../specifications/FEASIBILITY_MODEL.md) — time/memory verdicts
- [BENCHMARK_SPECIFICATION.md](../specifications/BENCHMARK_SPECIFICATION.md) — measurement (M4)
- [LIMITATIONS.md](../specifications/LIMITATIONS.md)

---

## 1. Pipeline overview

```
C++ source ──► Clang AST (-fsyntax-only) ──► findings with symbolic expressions        [M2, partial]
                                               │
constraints ───────────────────────────────────┼──► workload upper bounds per finding    [M3, partial]
                                               ├──► memory lower bounds                   [M3, partial]
                                               └──► feasibility (time: Unknown until M4)  [M3, partial]
benchmark runs ──► median, MAD, peak RSS, growth fit ──► runtime range                     [M4, planned]
```

No implemented path executes user code.

---

## 2. Quantities and what they are not

BigO Lens deals with distinct quantities. They are never substituted for one another, and the UI labels them
differently.

| Quantity | Definition | Produced by | Status today | Must not be presented as |
| --- | --- | --- | --- | --- |
| **Asymptotic complexity** | Growth class of a structural count as variables → ∞, e.g. `O(n^2)` (`time.asymptotic`) | Asymptotic derivation over the expression tree | Implemented | A count, a runtime, or a tight (Θ) bound — it is an upper bound (O) |
| **Structural count** | Number of executions of innermost loop bodies as a symbolic expression, e.g. `t * n^2` (`time.expression`); its value under constraints is the *workload* (`workload.upper_bound`) | Clang loop analyzer + workload evaluator | Implemented (partial) | CPU instructions, primitive operations, or runtime |
| **Estimated primitive operations** | Structural count weighted by the per-iteration cost of the body, including library calls via a cost model | STL cost model + body costing | Planned; the cost table exists as data only | — |
| **CPU instructions** | Retired machine instructions | Hardware performance counters | Not planned | Anything derived statically |
| **Measured time** | Wall-clock time of a run on the user's machine (median over repetitions) | Benchmark runner | Planned (M4); statistics library exists | A target-machine time |
| **Estimated target runtime** | Central value and range (low ≤ central ≤ high, ms) for the configured limit, derived from measurements | Growth fit + extrapolation | Planned (M4) | A guarantee |
| **Static allocation estimate** | Bytes of storage visible to static analysis (globals/statics exact, sized vectors at constraint maxima) | Storage analyzer + memory estimator | Implemented (partial) | Peak memory usage or an upper bound |
| **Measured peak RSS** | Peak resident set size of a benchmark run | Resource monitor | Planned (M4) | A worst case over all inputs |

Consequences:

- A workload of `4e10` means "at most 4·10¹⁰ executions of innermost loop bodies under these constraints". It does
  not say whether that takes 1 s or 100 s.
- Two solutions with the same O() can differ by orders of magnitude in workload; two with the same workload can
  differ by orders of magnitude in runtime.

---

## 3. Time complexity

Time is modelled per **loop nest**: each top-level loop in a function definition of the main file (template
primaries only, not instantiations) produces one `loop_nest` finding. Its `time` quantity contains:

| Field | Meaning |
| --- | --- |
| `expression` | Human-readable rendering of the structural count expression |
| `asymptotic` | `O(...)` string, or `null` if the expression shape is not supported by the derivation |
| `variables` | Free variables of the expression (source identifiers or `name.size()`) |
| `workload` | Evaluation under constraints ([§5](#5-constraint-aware-workload)) |

The structural count model:

- A straight-line body counts **1**.
- A body containing loops costs the **sum** of its child loops (sequential loops are summed; an assumption says so).
- A loop multiplies its trip count by the body cost or, when the body cost depends on the loop index and the index
  has unit step, forms a **summation** over the index range (triangular loops).
- Calls are **not** expanded: user-defined callees and library calls contribute nothing to the count and are listed
  in `limitations`.

Whole-program time is not summed across findings. `summary.max_time_workload` is the largest workload upper bound
among findings and `summary.dominant_finding_id` the finding that has it. It is a lower bound on the program's
total structural count, not a total.

---

## 4. Space complexity

Space is modelled per storage item:

| Item kind | Source construct | Size | Finding | Status |
| --- | --- | --- | --- | --- |
| `global_static` | Global or `static` variables | Exact bytes from Clang type layout | Static arrays also produce a `static_storage` finding (pattern `static-array`) | proven |
| `local_array` | Constant-size local array | Exact bytes; largest kept in `largest_local_array_bytes` | — | — |
| `vector_buffer` | `std::vector<T> v(count, ...)` with symbolic count | `count × sizeof(T)` expression | `dynamic_allocation` finding (pattern `sized-vector`) with a `space` quantity | estimated |

Only the element buffer of a sized vector is counted; nested ownership (e.g. `vector<vector<int>>`) is not followed.
Aggregation is described in [FEASIBILITY_MODEL.md §4](../specifications/FEASIBILITY_MODEL.md#4-memory-feasibility).
**Memory figures are lower bounds or estimated minima**; BigO Lens never claims a peak or an upper bound from static
analysis.

---

## 5. Constraint-aware workload

A structural count with free variables becomes a number only under constraints. The workload evaluator produces:

| Field | Meaning |
| --- | --- |
| `status` | `proven` (sound upper bound under the stated constraints), `estimated` (evaluated, worst case not guaranteed), `unknown` (cannot evaluate) |
| `upper_bound` | Number or `null` |
| `method` | `pointwise-maxima`, `sum-constraint-packing`, `sum-constraint-cases`, `pointwise-maxima-capped`, or `null` |
| `assumptions` | E.g. `'n' assumed non-negative (no lower bound configured)` |
| `missing_bounds` | Variables without a configured maximum |

The central case is the sum constraint: per-test-case O(N²) with T ≤ 10, N ≤ 2·10⁵, ΣN ≤ 2·10⁵ evaluates to 4·10¹⁰,
not 4·10¹¹. Full rules and worked examples: [CONSTRAINT_MODEL.md](../specifications/CONSTRAINT_MODEL.md).

A finding's `bound_status` (about the expression) and its workload `status` (about the evaluation) are independent:
a proven expression with a missing bound has `bound_status: proven` and `workload.status: unknown`.

---

## 6. Bound status and confidence

Two orthogonal axes.

**Bound status** — what kind of claim the expression is:

| Status | Meaning |
| --- | --- |
| `proven` | The expression is a sound upper bound on the structural count, under the listed assumptions |
| `estimated` | An expression exists but soundness is not guaranteed (bound variable may be modified, callee cost excluded) |
| `unknown` | The analyzer could not determine a count (e.g. data-dependent termination) |
| `unsupported` | The construct is recognised but deliberately not modelled (modified index, do-while, recursion) |

When statuses combine (nested loops), the **weakest** applies, in the order proven, estimated, unknown, unsupported.

**Confidence** — how much weight the user should give the finding. For `loop_nest` and `recursion` findings:

| Bound status | Condition | Confidence |
| --- | --- | --- |
| proven | no limitations and no early exit | `high` |
| proven | otherwise | `medium` |
| estimated | — | `low` |
| unknown / unsupported | — | `none` |

Storage findings (`static_storage`, `dynamic_allocation`) use a separate rule: `proven` → `high`, `estimated` →
`medium` (a sized-vector buffer is an exact `count × sizeof(T)` for the element buffer; it is `estimated` because
it is only part of the footprint).

Confidence never upgrades a status. A `proven` bound with `medium` confidence is still a sound upper bound on the
structural count; `medium` signals that library-call costs are excluded or that the loop may exit early.

---

## 7. Provenance

Every finding carries `provenance`, the analyzer that produced it:

| Provenance | Produces |
| --- | --- |
| `clang-loop-analyzer` | `loop_nest` findings |
| `clang-recursion-detector` | `recursion` findings |
| `clang-storage-analyzer` | `static_storage`, `dynamic_allocation` findings |

`engine.version`, `engine.protocol_version` and `engine.frontend` (Clang version string, or `null` when built
without the frontend) identify the producing engine. Benchmark-derived results (M4, planned) will carry their own
provenance and measurement parameters.

---

## 8. Unknown and unsupported states

- A finding with `bound_status` `unknown` or `unsupported` has `time: null`, confidence `none`, and its reason in
  `limitations` and `description`.
- Patterns: `data-dependent-loop` (unknown), `unsupported-loop` (unsupported), `direct-recursion` /
  `multiple-direct-recursion` (unsupported).
- These findings are always emitted and count toward `summary.finding_count`.
- `summary.coverage` is `partial` in protocol 1.0: the absence of a finding does not prove the absence of cost
  (for example, an expensive call outside any loop).
- The `unsupported_construct` category exists in the protocol for future use; current analyzers do not emit it.

---

## 9. Runtime estimation (planned, M4)

Not implemented. Design:

1. **Measure** the compiled program at progressively larger input sizes (never blindly at the maxima): median of
   repeated runs, MAD as variability, peak RSS.
2. **Fit** growth models (linear, linearithmic, quadratic) by least squares; select one only if it clearly beats the
   runner-up, otherwise report *ambiguous*. The fitting library exists today
   ([BENCHMARK_SPECIFICATION.md §8](../specifications/BENCHMARK_SPECIFICATION.md#8-growth-model-fitting-implemented)).
3. **Extrapolate** to the constrained size, producing `low ≤ central ≤ high` milliseconds, marked as extrapolated.
4. **Feed** the range to time feasibility (policy already implemented; see
   [FEASIBILITY_MODEL.md](../specifications/FEASIBILITY_MODEL.md)).

A selected growth model describes the measured range only and is never presented as proof of complexity. Static
complexity and measured growth are shown side by side; disagreement is surfaced, not silently reconciled.
