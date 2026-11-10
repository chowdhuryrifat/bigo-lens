# Performance

This document defines BigO Lens's performance goals, where time is spent, how performance is measured, and the hard limits that bound work on untrusted input.

> **No measured numbers are published here.** The project has not yet run a controlled performance study. Every figure below is either a hard limit enforced in code (marked *limit*) or a **target** that guides design (marked *target*). Targets are not guarantees. When measurements exist, they will be added along with the machine, build type and method used.

## 1. Goals

1. **Interactive analysis.** Running "Analyze Active C++ File" on a typical contest solution should feel interactive: results in roughly the time it takes to compile the file with `-fsyntax-only`.
2. **Bounded cost on any input.** No request can make the engine work without bound. Hostile or pathological inputs hit a limit and return a structured result.
3. **No editor impact.** The engine runs in a separate process, and the extension never blocks the extension host thread while waiting for it.
4. **Predictable benchmarks (M4).** When benchmarking exists, it must report measurement uncertainty and never present a single noisy run as a fact.

## 2. Where time goes

One `analyze` request does the following, in order:

1. **Process start-up.** The OS loads `bigolens-engine` and the Clang/LLVM shared libraries (`clang-cpp`, `LLVM`) when the build links them dynamically.
2. **Request read and validation.** The engine reads at most 4 MiB and runs a linear-time depth pre-check before the JSON parse. This cost is negligible next to the steps below.
3. **Clang parse with `-fsyntax-only`.** This step is **expected to dominate**. Its cost is driven by the headers the source includes, not by the user's code. `#include <iostream>` pulls in a large part of the standard library, and `#include <bits/stdc++.h>`, which is common in competitive programming, pulls in nearly all of it. A 30-line solution can therefore cost about as much to parse as a large translation unit.
4. **AST analysis.** The engine visits the function definitions in the main file only. Its cost is roughly linear in the size of the user's code, plus the size of the expression trees it builds.
5. **Workload evaluation.** Closed forms for affine summations, and iteration under a budget for non-affine summations.
6. **Serialization** of one JSON line.

Consequences for design:

- Optimizing analysis passes rarely matters. Optimizing **header parsing** does. The candidates are precompiled headers or a preamble for common headers, and reusing a long-lived engine process across requests. Both are **not implemented**, and tracked as future work in [IMPLEMENTATION_PLAN.md](../development/IMPLEMENTATION_PLAN.md).
- A Debug build (`dev` preset) of the engine is noticeably slower than a Release build (`release` or `ci`). Measure with Release builds only.

## 3. Hard limits (enforced in code)

| Limit | Value | Enforced in | On violation |
| --- | --- | --- | --- |
| Request size | 4 MiB | engine (`main.cpp`, `RequestHandler::handleText`) | `request_too_large` |
| Source content size | 2 MiB (`api::kMaxSourceBytes`) | engine request validation | `invalid_request` with path `/source/content` |
| JSON nesting depth | 64, checked before parsing | engine (`RequestHandler::handleText`) | `malformed_json` |
| Constraint integer magnitude | 10^18 | engine | `invalid_request` |
| `time_limit_ms` | 1 – 3,600,000 | engine | `invalid_request` |
| `memory_limit_mb` | 1 – 1,048,576 | engine | `invalid_request` |
| Collected compiler error diagnostics | 50 | Clang front end | Remaining diagnostics dropped |
| Evaluator budget for non-affine summations | 5×10^7 node evaluations (default) | `complexity::evaluate` | Evaluation stops with `EvalStatus::BudgetExceeded`, and no value is reported from it |
| Engine stdout read by the extension | 16 MiB | extension engine client | Process killed, error reported |
| Engine wall-clock time | `bigoLens.analysisTimeoutMs` (default 30,000 ms, allowed 1,000 – 600,000) | extension engine client | Process killed, timeout reported |
| Evaluator precision | IEEE double; values above 2^53 are approximate | `complexity::evaluate` | Non-finite values are reported as `Overflow` |

Changing a limit is a behavioural change. Update this table, the [JSON_PROTOCOL.md](../specifications/JSON_PROTOCOL.md) specification and the tests in the same pull request.

## 4. Budgets (targets)

These are design targets for a Release build on a typical developer laptop. They are **not** measured guarantees.

| Stage | Target | Notes |
| --- | --- | --- |
| `capabilities` request, end to end | Well under the time it takes to notice | No parsing involved |
| `analyze` of a contest-sized file, excluding header parsing | Small next to the Clang parse | Analysis is linear in user code |
| `analyze` end to end | Comparable to `clang++ -fsyntax-only` on the same file | Dominated by headers |
| Extension overhead (spawn, validation, rendering) | Small next to the engine time | ajv validators compiled once per session |
| Memory of the engine process | Comparable to `clang -fsyntax-only` on the same file | The AST dominates |

If a change makes a stage exceed its target by a clear margin, discuss it in the pull request with measurements.

## 5. Measurement approach

### 5.1 Engine

1. Build with the `release` preset: `cmake --preset release && cmake --build --preset release`.
2. Choose inputs from `tests/fixtures/cpp/`, plus at least one file that includes `<bits/stdc++.h>` when the toolchain provides it.
3. Measure the wall-clock time of the whole process, which includes start-up, with repeated runs. For example:

   ```bash
   for i in $(seq 1 20); do
     /usr/bin/time -f "%e s %M KiB" build/release/engine/bigolens-engine --request shared/examples/analyze.request.json > /dev/null
   done
   ```

   Or use a tool such as `hyperfine` if one is installed.
4. Report the **median** and a spread measure (MAD or min–max) over at least 10 runs. Do not report a single run. The `bigolens::benchmark::summarize` statistics (median, MAD, relative spread) follow the same convention.
5. Record the CPU, OS, compiler and LLVM version, the build type, and whether the libraries are linked dynamically.
6. To separate parsing from analysis, compare against `clang++ -fsyntax-only -std=c++20 <file>` on the same machine.

### 5.2 Extension

- Measure from the command invocation to the tree view update, using timestamps in the output channel during development. Do not keep timing logs in release builds unless a setting enables them.

### 5.3 Rules

- **No performance thresholds in ordinary CI.** Shared runners vary too much, so a threshold would make CI flaky without detecting regressions reliably.
- Performance pull requests (`perf:` commits) must include before and after measurements gathered as in §5.1.
- Never publish fabricated, estimated-as-measured or cherry-picked numbers (see [RULES.md §16](../../RULES.md#16-prohibited-practices)).

## 6. Benchmark performance (Planned M4)

The future benchmark runner measures *user programs*, not BigO Lens itself. It is planned to work as follows (see [BENCHMARK_SPECIFICATION.md](../specifications/BENCHMARK_SPECIFICATION.md)):

- Use warm-up runs, then repeated measurements at each of at least 4 distinct input sizes, which is the minimum `fitGrowth` accepts.
- Use robust statistics: median and MAD. Mark results with a high relative spread as noisy.
- Fit growth models (Linear, n log n, Quadratic). A model is selected only when its residual sum of squares is under half that of the runner-up and its slope is positive. Otherwise the result is reported as ambiguous.
- Use runtime estimates to make time feasibility something other than `unknown`. Until then, time feasibility is **always** `unknown`.
- Apply the same limits as any untrusted execution (see [THREAT_MODEL.md §4.2](THREAT_MODEL.md#42-malicious-benchmark-programs-tb6-planned-m4)).
