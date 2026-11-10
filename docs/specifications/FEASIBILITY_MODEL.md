# Feasibility Model

How BigO Lens classifies a solution against the configured time and memory limits. Implementation:
`engine/include/bigolens/feasibility/feasibility.hpp`, `engine/src/feasibility/feasibility.cpp` (`assessTime`,
`assessMemory`) and `engine/src/estimation/memory_estimator.cpp` (`estimateMemory`). Milestone **M3**, status
**partial**; time feasibility depends on **M4**.

Related: [ANALYSIS_MODEL.md](../design/ANALYSIS_MODEL.md) · [CONSTRAINT_MODEL.md](CONSTRAINT_MODEL.md) ·
[BENCHMARK_SPECIFICATION.md](BENCHMARK_SPECIFICATION.md)

---

## 1. Statuses

| Status | Protocol value | Meaning |
| --- | --- | --- |
| Likely safe | `likely_safe` | The pessimistic estimate is well within the limit (policy margin applies) |
| Borderline | `borderline` | The estimate range overlaps the region near the limit |
| High risk | `high_risk` | Even an optimistic estimate or a proven lower bound exceeds the limit |
| Unknown | `unknown` | Not enough information to classify. **Not** an implicit "safe" |

Each assessment carries a human-readable `reason`. Time and memory are reported as
`results.feasibility.time` and `results.feasibility.memory`.

## 2. Independence of time and memory

Time and memory are assessed **independently** and never combined into one verdict. A solution may be `high_risk`
on memory and `unknown` on time; the UI shows both. There is no "overall" feasibility field in protocol 1.0.

## 3. Time feasibility

Input: an optional runtime estimate `{ lowMs ≤ centralMs ≤ highMs }` and the optional `time_limit_ms`.

| Order | Condition | Result | Reason |
| --- | --- | --- | --- |
| 1 | No `time_limit_ms` | `unknown` | "no time limit configured" |
| 2 | No runtime estimate | `unknown` | "no runtime estimate available; structural workload alone does not determine runtime" |
| 3 | `lowMs > limit` | `high_risk` | "even the optimistic runtime estimate exceeds the time limit" |
| 4 | `highMs ≤ 0.5 × limit` | `likely_safe` | "the pessimistic runtime estimate is well within the time limit" |
| 5 | otherwise | `borderline` | "the runtime range overlaps the region near the time limit" |

The 50 % margin is `FeasibilityPolicy::timeSafeFraction = 0.5`: product policy, not a physical constant. It
absorbs machine differences and measurement noise that the range does not capture.

### 3.1 Why time is always Unknown today

The engine calls `assessTime` with **no runtime estimate**, because no component produces one: benchmarking (M4) is
not implemented, and the structural workload is deliberately **not** converted into milliseconds. A count of 4·10¹⁰
loop-body executions can mean very different runtimes depending on the body, memory access pattern, compiler and
machine; dividing by an assumed "operations per second" constant would present a guess as a measurement. Until M4,
`feasibility.time.status` is `unknown` for every analysis — with the second reason above when a time limit is
configured. The capabilities feature `time_feasibility` reports `requires_runtime_estimate`.

### 3.2 Future runtime model inputs (M4)

The runtime estimate passed to `assessTime` will be derived from:

| Input | Source |
| --- | --- |
| Measured medians and MAD per input size | Benchmark runner + `summarize` |
| Selected growth model or ambiguity | `fitGrowth` |
| Target size(s) | Constraint maxima, respecting sum constraints (packing shape) |
| Extrapolation distance | Largest measured size vs target |
| Machine factor | User-configured target-vs-local factor (no default assumed until calibrated) |
| Compiler and flags | Benchmark configuration |

Structural workload remains a separate, displayed quantity and may be used to choose target input shapes, never as
a runtime substitute. See [BENCHMARK_SPECIFICATION.md §9](BENCHMARK_SPECIFICATION.md#9-uncertainty-ranges-planned).

## 4. Memory feasibility

### 4.1 Memory estimate (`estimateMemory`)

| Field | Computation | Status |
| --- | --- | --- |
| `static_storage_bytes` | Sum of exact sizes of `global_static` items, saturating at 2⁶⁴ − 1 (`static_storage_saturated`) | Proven lower bound |
| `largest_local_array_bytes` | Maximum over `local_array` items | Informational; not added to the minimum |
| `estimated_minimum_bytes` | No sized vectors: `static_storage_bytes` (status `proven`). Sized vectors all evaluable: `static + Σ workload(count × sizeof(T))` (status `estimated`). Any vector not evaluable: `null` (status `unknown`) | See status column |

Each `vector_buffer` size is evaluated with `evaluateWorkload` under `singleInstanceConstraints(constraints)`
(sum-constrained variables capped at min(max, total), sum relations removed), so a buffer allocated per test case is
sized as one live instance rather than summed across cases
([CONSTRAINT_MODEL.md §9](CONSTRAINT_MODEL.md#9-aggregate-workload-calculation)). The `space` quantity of
`dynamic_allocation` findings uses the same single-instance constraints. Assumptions recorded:

- "sized std::vector buffers evaluated at constraint maxima, assuming each is constructed once with its largest
  single-instance size (sum constraints cap, but do not accumulate, per-case sizes); capacity slack and allocator
  overhead not included"
- "process baseline, stack frames, standard-library internals and unmodelled heap allocations are not included;
  figures are lower bounds, not peak usage"

### 4.2 Classification (`assessMemory`)

Input: the estimate, an optional known upper bound on peak memory, and `memory_limit_mb` (converted to bytes as
MiB). The engine currently passes **no** known upper bound.

| Order | Condition | Result |
| --- | --- | --- |
| 1 | No `memory_limit_mb` | `unknown` ("no memory limit configured") |
| 2 | `static_storage_bytes > limit` or saturated | `high_risk` ("static storage alone (… MiB) exceeds the limit") |
| 3 | `estimated_minimum_bytes > limit` | `high_risk` ("estimated minimum footprint (… MiB) exceeds the limit") |
| 4 | Known upper bound ≤ 0.8 × limit | `likely_safe` |
| 5 | Known upper bound ≤ limit | `borderline` |
| 6 | Known upper bound > limit | `high_risk` |
| 7 | No known upper bound | `unknown` ("only lower bounds are known; peak memory requires measurement or a complete allocation model") |

The 80 % margin is `FeasibilityPolicy::memorySafeFraction = 0.8`, product policy.

Consequence today: memory is either `high_risk` (a lower bound already exceeds the limit) or `unknown`. Static
analysis never yields `likely_safe` or `borderline`, because it cannot bound peak usage. Measured peak RSS (M4) will
not change this for arbitrary inputs either: it is a measurement for specific inputs, and its use as a "known upper
bound" will be labelled as such.

Note: rule 3 uses an `estimated` quantity. It assumes every sized vector is live at the same time at its largest
single-instance size. If two vectors are never live simultaneously the sum can still exceed the true peak; the
assessment reason names the estimated figure so the user can judge it.

## 5. Policy configuration

`FeasibilityPolicy` thresholds are compile-time defaults in protocol 1.0 and are not exposed in requests. Exposing
them would be an additive (minor) protocol change.
