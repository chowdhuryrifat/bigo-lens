# ADR 0009: Keep benchmarking as an independent subsystem

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Dynamic benchmarking (planned for M4; see [ADR 0006](0006-hybrid-analysis.md)) compiles and runs untrusted user code. It has very different concerns from static analysis: process control, resource limits, sandboxing, timing noise and statistics. If it were mixed into the engine core, the core's guarantee that **no user code is ever executed** would be harder to verify, and both parts would be harder to test.

## Decision

- Benchmarking lives in its own library, **`bigolens_benchmark`** (`benchmark/`, namespace `bigolens::benchmark`), with its own headers and tests. It does not depend on `bigolens_core` or Clang, and the engine does not depend on it yet.
- What exists today (Implemented): the pure statistical parts, with no I/O.
  - `statistics.hpp`: `summarize` (count, min, median, max, MAD, `relativeSpread`), which rejects empty, negative or non-finite samples.
  - `growth_fit.hpp`: `fitGrowth`, a least-squares fit of `t = a + b * f(n)` for `Linear`, `Linearithmic` and `Quadratic`. It needs at least 4 distinct positive sizes. A model is selected only when the best RSS is below 0.5 times the runner-up's and the slope is positive. `predict` is also provided.
- Planned (M4): the compiler adapter, input generators, runner and resource monitor. Until they exist, the protocol's `benchmark` action returns `not_implemented`.
- The planned execution rules: user-initiated only; a temporary working directory; timeouts and memory limits; process-tree cleanup; no shell; the compiler invoked with a fixed argv; platform isolation (Linux rlimits plus namespaces or seccomp where available; Windows Job Objects; macOS `sandbox-exec` is deprecated, so isolation there is an open problem). **A timeout alone is not a sandbox.** Until isolation exists, documentation must say that benchmarking is not sandboxed.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Benchmarking inside `bigolens_core` | Mixes code execution into the safe core and couples unrelated concerns. |
| Benchmarking in the TypeScript extension | Timing and process control are less precise. Logic would be duplicated if other clients appear. |
| Rely on an external benchmark framework (for example Google Benchmark) | Built for code you write yourself, not for running arbitrary programs on generated stdin under limits. |

## Consequences

**Positive**

- The static engine's no-execution guarantee stays easy to audit.
- The statistics are unit-tested in isolation (`statistics_test.cpp`, `growth_fit_test.cpp`).
- The security-sensitive runner can be reviewed and hardened on its own.

**Negative**

- An integration layer (the protocol `benchmark` action and conversion to `feasibility::RuntimeEstimate`) still has to be designed.
- An extra library target to maintain.

## Follow-up considerations

- Decide where the runner runs: a separate executable, or a mode of `bigolens-engine` that links `bigolens_benchmark`.
- Extend the growth models (for example log n, n^3, 2^n) with the same rule of not selecting a model when the data cannot discriminate.
- See [BENCHMARK_SPECIFICATION](../../specifications/BENCHMARK_SPECIFICATION.md) and [THREAT_MODEL](../../engineering/THREAT_MODEL.md).
