# Requirements

Functional (FR) and non-functional (NFR) requirements for BigO Lens. Each requirement has a stable ID, a priority,
the milestone that owns it, and its current status.

- **Priority:** `Must` (MVP blocker), `Should` (MVP target, may slip), `Could` (post-MVP).
- **Milestones:** M1 Foundation · M2 Static analysis · M3 Constraints & memory · M4 Dynamic benchmarking ·
  M5 VS Code integration · M6 Validation & packaging.
- **Status:** `Implemented` (behaviour exists and is tested), `Partial` (exists with documented gaps), `Planned`
  (not implemented).

Status reflects version 0.1.0 / protocol 1.0. See [ROADMAP.md](../development/ROADMAP.md) and
[DEFINITION_OF_DONE.md](../development/DEFINITION_OF_DONE.md).

---

## 1. Functional requirements

### 1.1 Engine and protocol

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-1 | The engine reads exactly one JSON request from a file (`--request FILE`) or stdin and writes exactly one JSON response line to stdout | Must | M1 | Implemented |
| FR-2 | The engine supports `--version` and `--help`; exit code 0 whenever a response was written (including error responses), 2 for usage errors, 1 otherwise | Must | M1 | Implemented |
| FR-3 | Requests and responses conform to `shared/schemas/request.schema.json` and `response.schema.json` (protocol 1.0) | Must | M1 | Implemented |
| FR-4 | The engine rejects unknown request fields with `invalid_request` and a JSON-pointer `path` | Must | M1 | Implemented |
| FR-5 | The engine accepts `schema_version` with the same major and a minor not newer than its own; otherwise `unsupported_schema_version` | Must | M1 | Implemented |
| FR-6 | The `request_id` is echoed in the response (or `null` when it could not be recovered) | Must | M1 | Implemented |
| FR-7 | The `capabilities` action reports actions, per-feature support levels and the STL cost table | Must | M1 | Implemented |
| FR-8 | The `benchmark` action returns `not_implemented` until M4 | Must | M1 | Implemented |
| FR-9 | An engine built without the Clang frontend returns `frontend_unavailable` for `analyze` and still serves `capabilities` | Must | M1 | Implemented |
| FR-10 | Source that fails to parse yields `source_parse_error` with compiler diagnostics (at most 50 error diagnostics); analysis never runs on an AST with errors | Must | M2 | Implemented |
| FR-11 | Internal failures produce an `internal_error` response; no exception escapes the engine | Must | M1 | Implemented |

### 1.2 Static analysis

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-20 | Each top-level loop nest in each function definition of the main file yields one `loop_nest` finding with id `F<k>` in source order | Must | M2 | Implemented |
| FR-21 | Canonical counted `for` loops (increasing and decreasing, unit and constant step) produce a trip-count expression | Must | M2 | Implemented |
| FR-22 | Index-dependent inner loops produce a summation (triangular loops) | Must | M2 | Implemented |
| FR-23 | Geometric (`*=`, `<<=`, `/=`, `>>=`) and square-root (`i * i <= n`) loops produce logarithmic / square-root trip counts | Must | M2 | Implemented |
| FR-24 | `while (t--)` and `while (t-- > 0)` are recognised as test-case loops | Must | M2 | Implemented |
| FR-25 | Countdown `while` loops with exactly one linear or dividing update are modelled | Should | M2 | Implemented |
| FR-26 | Range-for over a named container yields `v.size()`; over a constant-size array, its size | Must | M2 | Implemented |
| FR-27 | Status rules: modified index → unsupported; possibly modified bound → estimated; user-function calls → estimated; do-while → unsupported; unrecognised termination → unknown | Must | M2 | Implemented |
| FR-28 | Every finding lists assumptions, limitations and provenance | Must | M2 | Implemented |
| FR-29 | Bound status and confidence are reported as separate fields | Must | M2 | Implemented |
| FR-30 | Asymptotic `O()` upper bound derived from each expression (or `null` for unsupported shapes) | Must | M2 | Implemented |
| FR-31 | Direct recursion is detected and reported as `unsupported` with no time expression | Must | M2 | Implemented |
| FR-32 | Recurrence analysis for direct recursion produces a time expression | Should | M2 | Planned |
| FR-33 | Interprocedural analysis includes callee cost in loop nests | Should | M2 | Planned |
| FR-34 | The STL cost model is applied to per-iteration cost of library calls | Should | M2 | Planned (table exists, `data_only`) |
| FR-35 | Mutual (indirect) recursion is detected | Could | M2 | Planned |

### 1.3 Constraints, workload and memory

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-40 | Constraints support variable min/max, sum, product and linear constraints, time and memory limits | Must | M3 | Implemented (parsing and validation) |
| FR-41 | Constraint variables bind to source identifiers by name, or to `name.size()` | Must | M3 | Implemented |
| FR-42 | Workload is evaluated at maxima for monotone expressions and reported as `proven`; otherwise `estimated` | Must | M3 | Implemented |
| FR-43 | Missing upper bounds yield workload `unknown` with `missing_bounds` | Must | M3 | Implemented |
| FR-44 | Sum constraints are used for superadditive per-case costs by worst-case packing | Must | M3 | Implemented |
| FR-45 | Non-superadditive per-case costs fall back to a sound cases × per-case-maximum bound | Must | M3 | Implemented |
| FR-46 | Product and linear constraints tighten workload bounds | Should | M3 | Planned (validated only; an assumption is emitted) |
| FR-47 | Sum constraints over different count variables in one expression | Could | M3 | Planned (returns `unknown`) |
| FR-48 | Static storage (globals, statics) is summed exactly with saturation; largest local array tracked | Must | M3 | Implemented |
| FR-49 | Sized `std::vector<T>(count, …)` buffers are estimated as `count × sizeof(T)` | Must | M3 | Implemented |
| FR-50 | Memory feasibility reports `high_risk` when a lower bound exceeds the limit; `unknown` otherwise | Must | M3 | Implemented |
| FR-51 | Time feasibility classifies a runtime range against the limit (50% policy) | Must | M3 | Partial (policy implemented; always `unknown` until M4) |
| FR-52 | Nested container ownership and other heap allocations are modelled | Could | M3 | Planned |

### 1.4 Benchmarking (M4)

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-60 | Robust statistics over repeated measurements (min, median, max, MAD, relative spread), rejecting invalid samples | Must | M4 | Implemented (library) |
| FR-61 | Growth fitting for linear, linearithmic, quadratic models with an ambiguity rule | Must | M4 | Implemented (library) |
| FR-62 | Compile the solution with a configurable compiler and flags using a fixed argv and no shell | Must | M4 | Planned |
| FR-63 | Generate inputs from declared generators with recorded seeds | Must | M4 | Planned |
| FR-64 | Run with warm-up, repetitions, timeouts, memory limits and process-tree cleanup | Must | M4 | Planned |
| FR-65 | Progressive input scaling; never run directly at maxima | Must | M4 | Planned |
| FR-66 | Measure peak RSS | Must | M4 | Planned |
| FR-67 | Produce a runtime range (low/central/high) and feed time feasibility | Must | M4 | Planned |
| FR-68 | Platform isolation (Linux rlimits + namespaces/seccomp where available; Windows Job Objects) | Should | M4 | Planned |

### 1.5 VS Code extension

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-80 | Commands `bigoLens.analyzeActiveFile`, `bigoLens.editConstraints`, `bigoLens.showCapabilities` | Must | M1 | Implemented (scaffold) |
| FR-81 | Settings `bigoLens.enginePath`, `bigoLens.cppStandard`, `bigoLens.analysisTimeoutMs` | Must | M1 | Implemented |
| FR-82 | Engine location order: setting → `BIGOLENS_ENGINE_PATH` → bundled binary → development build | Must | M1 | Implemented |
| FR-83 | Per-file sidecar constraints `<file>.bigolens.json`, created from a template and validated before use | Must | M1 | Implemented |
| FR-84 | Engine spawned with `shell: false`, no arguments, request on stdin, stdout capped at 16 MiB, killed on timeout | Must | M1 | Implemented |
| FR-85 | Every response is validated against the shipped response schema | Must | M1 | Implemented |
| FR-86 | Tree view *Analysis Results* with findings, memory and feasibility summary; click navigates to source | Must | M1 | Implemented (scaffold) |
| FR-87 | Information-level diagnostics per finding in the Problems panel | Should | M1 | Implemented |
| FR-88 | Constraint form UI writing the sidecar file | Must | M5 | Planned |
| FR-89 | Webview report with explanations, strict CSP and nonce scripts | Should | M5 | Planned |
| FR-90 | Benchmark controls with explicit confirmation and cancel | Should | M5 | Planned |
| FR-91 | Problem-statement constraint extraction producing candidates that require user confirmation; offline heuristics; optional LLM never required | Could | After M5 | Planned |
| FR-92 | Stale-result marking after edits; optional CodeLens | Could | M5 | Planned |

### 1.6 Packaging

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| FR-100 | Tagged releases (`v*.*.*`) validate versions, build, test, and attach the Linux engine and `.vsix` to a GitHub Release | Must | M6 | Implemented (workflow) |
| FR-101 | Bundled engine binary inside the `.vsix` | Should | M6 | Planned |
| FR-102 | Validation corpus of solutions with known complexity | Must | M6 | Partial (fixtures in `tests/fixtures/cpp/`) |

---

## 2. Non-functional requirements

| ID | Requirement | Priority | Milestone | Status |
| --- | --- | --- | --- | --- |
| NFR-1 | **Correctness:** no result is reported with a stronger status than justified; unknown is preferred to wrong | Must | M2 | Implemented (status rules, tests) |
| NFR-2 | **Quantity separation:** complexity, structural count, runtime and memory are distinct fields and labels | Must | M1 | Implemented |
| NFR-3 | **Offline:** all implemented features work without network access | Must | M1 | Implemented |
| NFR-4 | **No mandatory LLM** | Must | M1 | Implemented (no LLM used) |
| NFR-5 | **Platform independence:** no dependency on any competitive programming platform | Must | M1 | Implemented |
| NFR-6 | **No implicit execution:** static analysis never executes user code | Must | M1 | Implemented |
| NFR-7 | **Input limits:** request ≤ 4 MiB, source ≤ 2 MiB, JSON depth ≤ 64 (checked before parsing), integers ≤ 10¹⁸ in magnitude | Must | M1 | Implemented |
| NFR-8 | **Fixed compiler arguments:** only the validated `cpp_standard` comes from the request | Must | M2 | Implemented |
| NFR-9 | **Core isolation from Clang:** `bigolens_core` has no Clang dependency; only `clang_frontend.cpp` includes Clang headers | Must | M1 | Implemented |
| NFR-10 | **Determinism:** identical request and engine build produce identical analysis results | Must | M2 | Implemented (no randomness in analysis) |
| NFR-11 | **Bounded evaluation:** symbolic evaluation has a node budget (default 5·10⁷) and reports overflow instead of returning non-finite values | Must | M3 | Implemented |
| NFR-12 | **Platforms:** Linux full build; macOS and Windows core engine; Clang frontend experimental on macOS/Windows | Must | M1 | Partial (see [LIMITATIONS.md](LIMITATIONS.md#15-platform-support)) |
| NFR-13 | **Build:** CMake ≥ 3.24, C++20; pinned dependencies with SHA-256 when fetched | Must | M1 | Implemented |
| NFR-14 | **CI:** build and test on Linux, macOS and Windows; extension lint, unit, smoke and integration tests | Must | M1 | Implemented |
| NFR-15 | **Static checks:** clang-format, clang-tidy, ESLint, Prettier, `-Werror` configure job | Should | M1 | Implemented |
| NFR-16 | **Security scanning:** CodeQL, dependency review, npm audit | Should | M1 | Implemented |
| NFR-17 | **Benchmark safety:** user-initiated, temp directory, timeouts, memory limits, process-tree cleanup, no shell; documented as not sandboxed until isolation exists | Must | M4 | Planned |
| NFR-18 | **Analysis latency:** analysis of a typical single-file solution completes within the extension timeout (default 30 000 ms) | Should | M2 | Partial (no measured target yet; see [PERFORMANCE.md](../engineering/PERFORMANCE.md)) |
| NFR-19 | **Accessibility:** status never conveyed by colour alone; theme colours only | Should | M5 | Partial |
| NFR-20 | **Protocol compatibility:** additive changes bump minor, removals or semantic changes bump major | Must | M1 | Implemented (policy) |
