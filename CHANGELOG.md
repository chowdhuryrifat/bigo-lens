# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).
The JSON protocol is versioned separately (see [JSON_PROTOCOL.md](docs/specifications/JSON_PROTOCOL.md)).

## [Unreleased]

### Added

- Linux benchmark runner with timeouts, resource limits (rlimits), process groups, and sample statistics (BL-404). Adds `crates/bigolens-engine/src/runner.rs` executing benchmark target binaries with monotonic wall-clock and CPU timeouts (`RLIMIT_CPU`), virtual memory ceiling (`RLIMIT_AS`), output caps (`RLIMIT_FSIZE`), file descriptor limits (`RLIMIT_NOFILE`), process count ceilings (`RLIMIT_NPROC`), and process-group isolation (`setpgid(0, 0)`) with an RAII kill-tree (`kill(-pgid, SIGKILL)`). Adds sample statistics in `crates/bigolens-core/src/statistics.rs` computing robust median, MAD, and relative spread across warmup and repeated runs, and reports honest `"not sandboxed"` transparency warnings until full Linux namespace isolation lands.
- Constraint-driven input generators (BL-403). Adds `crates/bigolens-core/src/generator.rs` with declarative input-format schemas (`SingleInteger`, `Array`, `TestCases`, `SumConstrainedCases`), a seeded 64-bit deterministic SplitMix64 pseudo-random generator, strict bound and byte-limit checking, and sum-constrained test case distribution.
- Fix clang-tidy readability braces in `clang_frontend.cpp` to ensure Code Quality CI passing on GitHub Actions.

- Compiler adapter with fixed argv (BL-402). Adds `crates/bigolens-engine/src/compiler.rs` discovering system C++ compilers (`clang++`, `g++`, `cl.exe`), compiling benchmark source with fixed argument vectors (validated C++ standards and optimization levels), flag escape protection (`--` and path prefixing), and structured diagnostic parsing from stderr without shell execution.

- Benchmark isolation architecture and specification (BL-401). Defines ADR 0014 selecting platform-specific isolation (Linux unprivileged user/mount/network/PID namespaces, seccomp-bpf, rlimits, and process-group kill-tree; Windows Job Objects with restricted tokens; macOS process limits with explicit "not sandboxed" labelling; fail-closed policy requiring explicit `allow_unsandboxed: true`). Finalizes the Protocol 1.1 `benchmark` action request and response schema in `BENCHMARK_SPECIFICATION.md` and updates `THREAT_MODEL.md`.

- Multi-language complexity analysis expansion in Rust (`bigolens-syntax`). Expands language support to compute algorithmic complexity and memory allocations regardless of source language across C++, Python, Java, Rust, and JavaScript/TypeScript:
  - `java.rs`: parses Java standard `for` loops, enhanced `for` loops (`arr.length`), logarithmic `while` loops, recursive divide-and-conquer (`mergeSort`) and linear decrement methods, and array/heap allocations (`new int[n]`).
  - `rust_lang.rs`: parses Rust range `for` loops (`0..n`, `0..=n`), triangular nested loops, halving `while` loops, recursive functions (`merge_sort`), and vector allocations (`vec![0; n]`, `Vec::with_capacity(n)`).
  - `javascript.rs`: parses JavaScript/TypeScript standard `for` loops, `for..in`/`for..of` loops, halving `while` loops, recursive functions, and heap allocations (`new Array(n)`).
  - `cpp.rs`: adds C++ recursion analysis and dynamic heap allocation analysis (`new int[n]`).
  - Updates `request.schema.json` to allow all supported language identifiers in `source.language`.
  - Updates VS Code extension activation events and language normalization to support analyzing Java, Rust, JavaScript, and TypeScript files directly from the editor.

- Map validation errors to positions in sidecar files (BL-307). When ajv or the engine reports validation errors or inverted ranges (`{min: 10, max: 1}`) with a JSON-pointer path under `/constraints`, the extension maps the pointer using an RFC 6901 JSON scanner to source lines/columns and publishes diagnostics directly on `<file>.bigolens.json`. Underlines keys, handles array indices, and decodes escaped pointer tokens.

- End-to-end sum-constrained extension integration tests (BL-306). Adds end-to-end integration and smoke test coverage for `09_sum_constrained_test_cases.cpp` with a sidecar constraints file ($t \le 10, n \le 2\cdot 10^5, \sum n \le 2\cdot 10^5$), verifying $4\cdot 10^{10}$ workload derivation with the `sum-constraint-packing` method.
- Memory estimates for node-based containers (BL-305). Loops inserting into `std::set` or `std::map` report estimated memory based on libstdc++/libc++ 32-byte node overhead plus value size, marked as `Estimated` with an explicit assumption string (noting upper bounds on conditional insertions and excluded allocator overhead).
- Rust engine architecture (ADR 0012). Re-architects the standalone engine as a modular Cargo workspace in `crates/`:
  - `crates/bigolens-core`: Pure Rust analysis core (`#![forbid(unsafe_code)]`) with domain models, symbolic expression trees (`Arc<Expr>`), numerical evaluator, constraints optimizer, memory model, and feasibility assessment.
  - `crates/bigolens-engine`: CLI binary using `clap` providing JSON protocol IPC matching the versioned JSON protocol 1.0.
  - Updates `RULES.md`, `AGENTS.md`, `ARCHITECTURE.md`, `SYSTEM_DESIGN.md`, and `CODING_STANDARDS.md` to establish Rust engineering rules and toolchain standards.
- Self-contained Linux x64 `.vsix` (BL-108). The engine is linked statically against Clang/LLVM
  (`BIGOLENS_STATIC_CLANG`, `package` preset) and ships Clang's builtin headers, which the engine finds next to
  its executable. Adds `scripts/build/package-extension.sh`, `scripts/test/verify-vsix.sh` and the CI job
  `package-linux` (ADR 0011).
- `bigolens-engine --version` prints the Clang resource directory in use.
- The VS Code smoke suite can run an end-to-end analysis (`BIGOLENS_E2E_REQUIRE_ENGINE=1`) against an unpacked
  `.vsix` (`BIGOLENS_E2E_EXTENSION_PATH`).
- Project foundation: CMake build with presets, C++20 native engine, TypeScript VS Code extension, documentation,
  GitHub Actions workflows, issue and pull request templates.
- JSON protocol 1.0 with request/response JSON schemas, request validation in the engine and response validation
  in the extension; example requests with responses generated by the engine.
- Engine CLI `bigolens-engine` with request size, source size and nesting-depth limits; every failure is reported as
  a structured error response.
- Symbolic expression trees, a numeric evaluator with overflow and budget detection, and asymptotic (O) derivation.
- Clang LibTooling frontend (partial): counted, nested, sequential, triangular, logarithmic and square-root `for`
  loops; `while (t--)` test-case loops; countdown/halving `while` loops; range-based `for`; direct recursion
  detection (reported as unsupported); global/static storage, local arrays and sized `std::vector` buffers.
- Constraint workload evaluation with variable bounds and sum constraints across test cases.
- Conservative memory estimate (lower bounds) and time/memory feasibility classification.
- STL operation cost table (data only).
- Benchmark statistics (median, MAD) and growth-model fitting (linear, linearithmic, quadratic) library.
- VS Code commands to analyze the active file, edit a constraints sidecar file and show engine capabilities; an
  Analysis Results tree view and Problems-panel diagnostics.

- Loop-form coverage fixtures (BL-201): `tests/fixtures/cpp/forms/` has positive and near-miss cases for every
  supported form and status rule, checked by the table-driven `tests/integration/loop_forms_test.cpp`.

- Recurrence extraction for direct recursion (BL-202). The extracted recurrence is reported first in the
  finding's limitations, e.g. `T(n) = 2 T(n/2) + O(n), where n = hi - lo` for merge sort. Adds the solver interface
  `complexity::RecurrenceSolver` (no solvers yet) and the capability `recurrence_extraction: partial`.
- `complexity::substitute` for symbolic expressions.
- Common recurrence solving (BL-203). Solves divide-and-conquer recurrences `T(n) = a·T(n/b) + Θ(n^k (log n)^p)`
  via `MasterTheoremSolver` and linear decrement recurrences `T(n) = T(n - c) + f(n)` via `LinearDecrementSolver`.
  Recursive binary search resolves to `O(log n)` (Proven), merge sort to `O(n log n)` (Proven), and recursive linear sum
  to `O(n)` (Proven). Multiple subtractive recursion (Fibonacci) remains unsupported with an explicit limitation.
  Updates the `capabilities.features.recursion_analysis` value from `unsupported` to `partial`.
- Interprocedural cost propagation for non-recursive callees (BL-204). LoopAnalyzer analyzes function bodies of
  non-recursive callees defined in the main file and substitutes actual arguments into the callee cost, reporting
  `O(m n)` when a loop over `n` calls an `O(m)` helper, and composing summations when the loop index is passed.
  Recursive callees and mutual call cycles fall back to Estimated with explicit limitations.
  Updates `capabilities.features.interprocedural_analysis` value from `unsupported` to `partial`.
- STL cost model application to recognized container operations and top-level algorithms (BL-205).
  Applies asymptotic costs from `estimation::stlCostTable()` (`std::vector`, `std::map`, `std::set`, `std::priority_queue`,
  `std::unordered_map`, `std::sort`, `std::lower_bound`) to calls in loop nests and top-level algorithms, binding
  container sizes from constructors, pointer/iterator offsets, parameters, or loop trips. Hash containers report average
  $O(1)$ with `estimated` status and hash-collision limitation. Handled STL calls are no longer listed as unincluded costs.
  Updates `capabilities.features.stl_cost_model` value from `data_only` to `partial`.
- Two-pointer while loop analysis (BL-206). Recognizes two-pointer convergence loops (`while (l < r)`, `while (r > l)`,
  `while (l <= r)`, `while (r >= l)`) where every path through the body performs exactly one of `++l`/`l++`/`l += 1` or
  `--r`/`r--`/`r -= 1`, and nothing else writes to `l` or `r`. Resolves `l0` and `r0` from declarations, assignments, or
  parameters, yielding $O(n)$ Proven bounds. Binary search and data-dependent termination shapes remain Unknown.
- M2 specifications, capabilities, and examples synchronization (BL-207). Synchronized engine capabilities
  (`recursion_analysis: partial`, `interprocedural_analysis: partial`, `stl_cost_model: partial`) across protocol schemas,
  architecture documents, limitations, and regenerated example response fixtures.
- Product constraint workload tightening (BL-301). Tightens monotone product expressions under product constraints
  (e.g. `n·m ≤ 10^6` on grid loops with `n, m ≤ 10^6`), giving 10^6 with method `product-constraint-bound` and status
  `proven`. Soundly bounds higher-power products (such as `n²·m` to 10^12) by substituting the product bound and
  evaluating remaining factors at individual maxima. Non-monotone expressions and expressions where variables are not
  multiplied together fall back with an assumption. Removes the "product constraint not used" assumption when applied.
- Linear constraint workload tightening (BL-302). Tightens workloads for monotone expressions under linear constraints
  (Σ cᵢ·xᵢ ≤ B with non-negative coefficients). Iteratively tightens individual variable maxima, bounds linear combinations
  in addition, and solves closed-form constrained optimization for products along line segments (e.g. `n·m` with `n + m ≤ 2·10^5`
  gives 10^10, Proven, method `linear-constraint-bound`). Negative coefficients and non-monotone expressions emit an assumption
  and fall back soundly.
- Multiple sum constraints and separable packing (BL-303). Supports multiple sum constraints sharing one count variable
  (e.g. `Σn ≤ S₁` and `Σm ≤ S₂` over `t`). When the per-case cost is separable into additive terms depending on at most one
  sum-constrained variable, each term is packed independently, giving exact tight bounds (e.g. `t·(n + m)` evaluates to
  `S₁ + S₂`, Proven, method `sum-constraint-packing`). Interacting non-separable shapes fall back soundly to
  `sum-constraint-cases`, while sum constraints with different count variables remain Unknown with an explicit limitation.
- Memory estimates for nested vectors and sized strings (BL-304). Extends storage analysis in the C++ engine to recognize
  2D vectors (e.g. `std::vector<std::vector<T>> v(n, std::vector<T>(m))`), reporting symbolic bytes
  `n·m·sizeof(T) + n·sizeof(vector<T>)` using Clang type layout, and sized `std::string` constructions (`std::string s(n, 'c')`)
  reporting symbolic bytes `n`. Both emit `Estimated` bounds with explicit exclusions for allocator overhead and container
  bookkeeping. Triggers `high_risk` memory feasibility when evaluations exceed configured memory limits.

### Changed

- Development and CI tooling now require Node.js 22 (Node 20 reached end of life). Upgraded ESLint 10,
  `@eslint/js` 10, `@vscode/test-electron` 3, `@vscode/vsce` 4 and `@types/node` 22. Test scripts use
  `node --test` glob patterns, which Node 22 requires.

### Fixed

- `while` loops with an unbraced single-statement body (`while (x != 0) x /= 10;`) are now recognised. They were
  reported as Unknown.
- Range-for loops over user-defined containers are no longer downgraded to Estimated because of the implicit
  `begin()`/`end()` calls in the loop header.

- The packaged extension now contains its runtime dependency `ajv`. It was previously packaged with
  `--no-dependencies` and would have failed to activate when installed from a `.vsix`.
- The extension restores the executable bit of the bundled engine if archive extraction dropped it.

### Not yet implemented

- Runtime estimation and the benchmark runner (the `benchmark` action returns `not_implemented`).
- Map/set memory estimations (BL-305) and benchmarking runner (M4).
