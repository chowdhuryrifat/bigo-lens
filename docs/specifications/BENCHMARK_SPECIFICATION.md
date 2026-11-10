# Benchmark Specification (M4)

Specification of dynamic benchmarking: compiling the user's solution, generating inputs, measuring runtime and peak
memory, fitting growth models, and producing a runtime range for time feasibility.

> **Status.** Milestone **M4 Dynamic benchmarking** — **not implemented** except for two library components in
> `benchmark/`:
>
> | Component | Files | Status |
> | --- | --- | --- |
> | Sample statistics | `benchmark/include/bigolens/benchmark/statistics.hpp`, `benchmark/src/statistics.cpp` | Implemented, unit-tested |
> | Growth-model fitting | `benchmark/include/bigolens/benchmark/growth_fit.hpp`, `benchmark/src/growth_fit.cpp` | Implemented, unit-tested |
> | Compiler adapter, input generators, runner, resource monitor, isolation | — | Not implemented |
>
> The `bigolens_benchmark` library does not compile or run any code. The engine's `benchmark` action returns
> `not_implemented`. **Until platform isolation exists, benchmarking must be documented and presented as not
> sandboxed.**

Related: [SECURITY.md](../../SECURITY.md) · [THREAT_MODEL.md](../engineering/THREAT_MODEL.md) ·
[FEASIBILITY_MODEL.md](FEASIBILITY_MODEL.md) · [ANALYSIS_MODEL.md §9](../design/ANALYSIS_MODEL.md#9-runtime-estimation-planned-m4)

---

## 1. Principles

1. **User-initiated only.** A benchmark runs only on an explicit user action for a specific file. Analysis never
   triggers execution.
2. **Measurements are never fabricated or repaired.** Invalid samples cause the summary to fail, not to be patched.
3. **Never run blindly at the maxima.** Sizes grow progressively under a time budget.
4. **Fitted models describe the measured range only.** They are not a proof of complexity and extrapolation is
   always labelled.
5. **Reproducible:** every run records the parameters needed to repeat it.

---

## 2. Compilation

| Item | Specification |
| --- | --- |
| Compiler | Configurable executable path (e.g. `g++`, `clang++`, MSVC `cl`); resolved to an absolute path before use |
| Flags | Configurable list; default proposal `-O2 -std=<cpp_standard>`; stored as an array, never a string |
| Invocation | Fixed argv built by the engine: `[compiler, ...flags, <source>, -o, <output>]`; **no shell**, no argument splitting, no environment-variable expansion |
| Working directory | Fresh temporary directory per benchmark, removed afterwards |
| Limits | Compile timeout and memory limit; process-tree cleanup on timeout |
| Output | Compiler diagnostics returned verbatim as protocol diagnostics; non-zero exit → structured error |
| Recorded | Compiler path, `--version` output, flags, source hash |

## 3. Input generation

| Item | Specification |
| --- | --- |
| Generators | Declarative built-ins parameterised by the constrained variables (e.g. "t test cases, each an array of n values in [lo, hi]"); user-supplied generator programs are a later extension subject to the same isolation |
| Constraint respect | Every generated input satisfies the configured constraints, including sum constraints (Σn ≤ S); for sum-constrained inputs the generator uses the worst-case packing shape from [CONSTRAINT_MODEL.md §9.2](CONSTRAINT_MODEL.md#92-sum-constraint-packing-sum-constraint-packing) unless configured otherwise |
| Determinism | Seeded PRNG with a documented algorithm; seed recorded |
| Delivery | Input written to a file in the temporary directory and connected to the child's stdin |
| Size | Inputs above a configurable byte cap are refused |

## 4. Warm-up and repeated measurements

| Item | Specification |
| --- | --- |
| Warm-up | `w ≥ 0` unmeasured runs per size (default proposal 1) to populate caches and page in the binary |
| Repetitions | `r` measured runs per size (default proposal 7; minimum 3) |
| Clock | Monotonic wall clock around process start to exit; CPU time recorded separately where the platform provides it |
| Output | stdout discarded to a file with a size cap; stderr captured with a size cap |
| Failure | A run that crashes, times out or exceeds memory is recorded as such and is never included as a timing sample |

## 5. Median and variability (implemented)

`summarize(samples)` returns `SampleSummary { count, minimum, median, maximum, mad, relativeSpread }`:

- `median`: middle value (mean of the two middle values for even counts).
- `mad`: median absolute deviation from the median, unscaled (no normal-consistency factor such as 1.4826 is
  applied).
- `relativeSpread = mad / median` (0 when the median is 0).
- Returns `std::nullopt` for an empty input or if any sample is negative or non-finite.

Policy (planned): a size whose `relativeSpread` exceeds a threshold (proposal 0.1) is re-measured once; if still
noisy it is kept but flagged and widens the uncertainty range (§9).

## 6. Peak RSS

| Platform | Mechanism (planned) |
| --- | --- |
| Linux | `wait4()` / `getrusage(RUSAGE_CHILDREN)` `ru_maxrss` (KiB) for the child process tree; cgroup `memory.peak` where available |
| macOS | `wait4()` `ru_maxrss` (bytes on macOS) |
| Windows | Job Object `PeakProcessMemoryUsed` / `PeakJobMemoryUsed` |

Reported as **measured peak RSS** for that input, a separate quantity from the static allocation estimate
([ANALYSIS_MODEL.md §2](../design/ANALYSIS_MODEL.md#2-quantities-and-what-they-are-not)). Never presented as a worst
case over all inputs.

## 7. Progressive input scaling

The benchmark never starts at the constrained maxima.

1. Start with a small size `n₀` (proposal: the larger of the variable's `min` and 1000, capped at `max`).
2. Multiply the size by a factor (proposal 2) each step, keeping all other variables and relationships valid.
3. Stop when any of: the median exceeds a per-size budget (proposal 1 s); the total benchmark budget is exhausted;
   the constrained maximum is reached; a run fails or exceeds the memory cap.
4. At least 4 distinct sizes are required for fitting (§8). If fewer are obtained, report measurements without a
   fit.
5. Running at the maximum is allowed only when the predicted time for it is within the per-size budget.

## 8. Growth-model fitting (implemented)

`fitGrowth(points, options)` fits `t(n) = a + b · f(n)` by ordinary least squares for each candidate:

| Model | `f(n)` |
| --- | --- |
| `linear` | `n` |
| `linearithmic` | `n · log₂(max(n, 2))` |
| `quadratic` | `n²` |

Rules (as implemented):

- Input points are `{n, milliseconds}` (median per size). Returns `std::nullopt` if any `n ≤ 0`, any time `< 0`, any
  value non-finite, or fewer than `max(minimumPoints, 3)` distinct sizes (`minimumPoints` default 4).
- Features are scaled by their maximum to keep the normal equations well conditioned.
- Each fit reports `intercept`, `slope`, `rss` (residual sum of squares) and `r2`.
- Fits with a **negative slope** are discarded (runtime decreasing with `n` is implausible for these models).
- Remaining fits are sorted by RSS, best first. If every fit was discarded, the result has no fits, no selection
  and `ambiguous = true`.
- A model is **selected** only if the best fit has slope `> 0` **and** (it is the only fit **or** its RSS is
  `< discriminationRatio × runner-up RSS`, `discriminationRatio` default 0.5). Otherwise `ambiguous = true` and no
  model is selected. Flat data is always ambiguous.
- `predict(fit, n)` evaluates `a + b · f(n)`; extrapolation must be labelled by the caller.

Planned extensions: additional models (`n^0.5`, `n log² n`, `n³`, `2^n`) only with tests showing they do not make
selection unstable; per-model confidence from residual structure.

## 9. Uncertainty ranges (planned)

The runtime estimate is `low ≤ central ≤ high` milliseconds at the target size:

- **Central:** `predict(selected, n_target)`, or the measured median if `n_target` was measured.
- **Range:** the envelope of (a) predictions of all fits within the discrimination ratio of the best (when
  ambiguous, all non-discarded fits), (b) predictions using per-size `median ± MAD` re-fits, and (c) a configurable
  machine-difference factor for the target (no default value is assumed until calibrated and documented).
- Extrapolation beyond the largest measured size widens the range and sets an `extrapolated` flag.
- If no fit is available, no runtime estimate is produced and time feasibility remains `unknown`.

The range is the input to `assessTime` ([FEASIBILITY_MODEL.md §3](FEASIBILITY_MODEL.md#3-time-feasibility)).

## 10. Resource limits

| Limit | Applies to | Enforcement (planned) |
| --- | --- | --- |
| Wall-clock timeout | Compile, each run | Timer; kill the whole process tree |
| CPU time | Each run | `RLIMIT_CPU` (POSIX), Job Object (Windows) |
| Memory | Each run | `RLIMIT_AS` / cgroup (Linux), Job Object memory limit (Windows); macOS best effort |
| Processes | Each run | `RLIMIT_NPROC` / cgroup `pids.max` (Linux), Job Object active-process limit (Windows) |
| File size | Each run | `RLIMIT_FSIZE` (POSIX); output caps |
| Total benchmark budget | Whole benchmark | Engine-side deadline |

A timeout alone is not a sandbox.

## 11. Isolation per platform (planned)

| Platform | Mechanism | Notes |
| --- | --- | --- |
| Linux | rlimits; user/mount/network/PID namespaces and seccomp where available; no network | Fallback to rlimits only is labelled "not sandboxed" |
| macOS | rlimits only | `sandbox-exec` is deprecated and not relied upon; labelled "not sandboxed" |
| Windows | Job Objects (kill-on-close, memory, process count); restricted token where feasible | Labelled according to what is actually applied |

The UI shows the isolation actually applied for each run
([UI_UX_DESIGN.md §13](../design/UI_UX_DESIGN.md#13-benchmark-controls-future-m4--m5)).

## 12. Reproducibility

Each benchmark result records: engine version, compiler path and version, flags, source hash, generator name and
parameters, seed, sizes, warm-up and repetition counts, per-run raw samples, summaries, fit results, platform, CPU
model string where available, and the isolation level applied. Re-running with the same record on the same machine
must reproduce inputs exactly; timings are expected to vary within the reported spread.

## 13. Protocol (planned)

The `benchmark` action exists in protocol 1.0 and currently returns `not_implemented`. Benchmark options and result
fields will be added as a **minor** protocol version bump (additive) per
[JSON_PROTOCOL.md §8](JSON_PROTOCOL.md#8-compatibility-and-versioning).
