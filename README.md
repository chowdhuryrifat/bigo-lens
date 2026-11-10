# BigO Lens

**Constraint-aware C++ complexity analysis for VS Code.** BigO Lens combines Clang AST static analysis, symbolic
workload estimation under problem constraints, and memory feasibility analysis. Benchmarking is planned. It is built
for competitive programmers who want to know whether an algorithm fits the actual limits of a problem.

> **Project status: early development (v0.1.0, milestone M1 — Foundation).** The native engine, the JSON protocol, a
> partial Clang-based loop analyzer, the constraint workload evaluator and a VS Code extension scaffold work and are
> tested. Runtime estimation, benchmarking, recursion analysis and the full UI are not implemented yet. See
> [Implementation status](#what-works-today).

## The problem

A complexity class alone does not answer the question a competitive programmer actually has:

> T ≤ 10, N ≤ 200 000, ΣN ≤ 200 000, time limit 2 s. My solution is O(N²) per test case. Does it fit?

Multiplying the independent maxima gives T × N² = 4·10¹¹, but the sum constraint means all ten test cases cannot be
maximal at once. The true worst case is one test of size 200 000: 4·10¹⁰ loop-body executions. That is still far
too slow, but for the right reason and by the right amount. BigO Lens is designed to reason about these
relationships and to explain each result: the source location, the symbolic expression, the assumptions, the bound
status and the confidence.

## What works today

| Capability | Status |
| --- | --- |
| Clang LibTooling frontend (in-memory parse, compiler diagnostics, no code execution) | Implemented |
| Loop nests: counted `for` loops (linear, nested, sequential, triangular/dependent, `*=`/`/=` logarithmic, `i*i<=n`), `while (t--)`, countdown/halving `while`, range-`for` | Implemented (partial coverage) |
| Symbolic expression trees (constant, variable, +, ×, power, log, summation, max), numeric evaluator, O() derivation | Implemented |
| Constraint workload: variable bounds, **sum constraints across test cases** (exact packing for superadditive costs) | Implemented |
| Product and linear constraints | Parsed and validated; not yet used to tighten bounds |
| Static memory: global/static storage (exact), local arrays, sized `std::vector` buffers | Implemented (lower bounds only) |
| Recursion | Detected and reported as *Unsupported*; recurrences are not solved |
| STL cost model (vector, map, set, unordered_map average/worst, priority_queue, sort, lower_bound) | Data table only; not yet applied to findings |
| Time feasibility | Always *Unknown* until a runtime estimate exists (needs benchmarking, M4) |
| Benchmarking | Statistics and growth-model fitting library only. The `benchmark` action returns `not_implemented` |
| VS Code: commands, results tree view, Problems-panel diagnostics, constraints sidecar file | Scaffold implemented |

Unsupported or data-dependent code (for example binary-search `while (lo < hi)` loops) is reported as **Unknown**
or **Unsupported**. BigO Lens does not guess.

## Example

```cpp
int t; std::cin >> t;
while (t--) {
  int n; std::cin >> n;
  for (int i = 0; i < n; ++i)
    for (int j = 0; j < n; ++j) acc += i * j;
}
```

With constraints `t ≤ 10`, `n ≤ 200000`, `Σn ≤ 200000` the engine reports (abridged from
[`shared/examples/analyze.response.json`](shared/examples/analyze.response.json), which is real engine output):

```text
F1  test-case-loop  main()  line 9
    time expression   t * n * n        asymptotic O(n^2 t)
    workload          ≤ 4e10  [proven, method sum-constraint-packing]
    bound status      proven           confidence high
```

The *time expression* is a structural count of innermost loop-body executions. It is not a CPU instruction count and
not a runtime; [ANALYSIS_MODEL.md](docs/design/ANALYSIS_MODEL.md) explains the distinction.

## Architecture

```text
VS Code extension (TypeScript)  ──  versioned JSON over stdin/stdout  ──  bigolens-engine (C++20)
  commands · tree view · diagnostics                                       ├─ protocol validation
  engine locator · schema validation (ajv)                                 ├─ Clang frontend (only Clang-aware module)
                                                                           ├─ symbolic expressions · O() derivation
                                                                           ├─ constraint workload engine
                                                                           ├─ memory estimator · feasibility
                                                                           └─ (M4) benchmark subsystem
```

The engine runs as one process per request, and Clang types never cross the protocol boundary. See
[ARCHITECTURE.md](ARCHITECTURE.md) and the [architecture decision records](docs/architecture/DECISIONS/README.md).

## Technology stack

C++20 · Clang LibTooling · CMake ≥ 3.24 · nlohmann/json · GoogleTest/CTest · TypeScript · VS Code Extension API ·
Node.js 20 · ajv · ESLint · Prettier · clang-format · clang-tidy · GitHub Actions · CodeQL · Dependabot.

## Installation status

BigO Lens is **not published** to the VS Code Marketplace. A self-contained **Linux x64 `.vsix`** can be built with
`scripts/build/package-extension.sh`. It bundles a statically linked engine, so LLVM does not need to be installed;
only the C++ standard library headers (`g++`) are required. CI also uploads it as the `bigo-lens-vsix-linux-x64`
artifact. Alternatively, build the engine and run the extension from source in an Extension Development Host. See
[Getting started](docs/user/GETTING_STARTED.md).

## Building

Prerequisites: CMake ≥ 3.24, a C++20 compiler, Node.js ≥ 20 and npm. The Clang frontend also needs the
Clang/LLVM development packages, for example on Ubuntu:
`sudo apt install cmake ninja-build clang-18 llvm-18-dev libclang-18-dev`.

```bash
cmake --preset dev                 # add -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang if Clang is not found
cmake --build --preset dev
ctest --preset dev

cd extension
npm ci
npm run compile
```

Without Clang, use the `no-clang` preset. That builds the core engine, which answers `capabilities` and reports
`frontend_unavailable` for `analyze`. Then open the repository in VS Code and press **F5** ("Run Extension").

Try the engine directly:

```bash
./build/dev/engine/bigolens-engine --request shared/examples/analyze.request.json
```

## Testing

| Suite | Command |
| --- | --- |
| Native unit + integration tests (GoogleTest, CTest) | `ctest --preset dev` |
| Extension unit tests (node:test) | `cd extension && npm test` |
| Extension ↔ real engine | `npm run test:integration` |
| VS Code extension-host smoke test | `npm run test:vscode` (downloads VS Code; needs a display) |
| JSON schema and example validation | `npm run validate:schemas` |
| Lint / format | `npm run lint`, `npm run format:check`, `scripts/development/format.sh --check` |

The [testing strategy](docs/engineering/TESTING_STRATEGY.md) covers the regression fixtures in
`tests/fixtures/cpp/` and the negative tests.

## Platform support

| Platform | Core engine | Clang frontend |
| --- | --- | --- |
| Linux (Ubuntu, LLVM 18 in CI) | Supported | Supported |
| macOS | Built and tested in CI | Experimental (Homebrew LLVM, allowed to fail) |
| Windows (MSVC) | Built and tested in CI | Experimental / untested |

## Roadmap

M1 Foundation → M2 Static analysis → M3 Constraints & memory → M4 Dynamic benchmarking → M5 VS Code integration →
M6 Validation & packaging. See [ROADMAP.md](docs/development/ROADMAP.md) and the issue backlog in
[IMPLEMENTATION_PLAN.md](docs/development/IMPLEMENTATION_PLAN.md).

## Limitations

Static complexity analysis is undecidable in general. BigO Lens analyzes a deliberately small set of patterns and
says so when it cannot. It does not include callee costs (interprocedural analysis), solve recurrences, apply STL
costs to findings, or produce runtime predictions yet. Memory figures are lower bounds, never peak usage. Constraint
variables bind to source identifiers *by name*. See [LIMITATIONS.md](docs/specifications/LIMITATIONS.md).

## Documentation

Start with [docs/README.md](docs/README.md). Key documents: [DESIGN.md](DESIGN.md) ·
[ARCHITECTURE.md](ARCHITECTURE.md) · [RULES.md](RULES.md) ·
[JSON protocol](docs/specifications/JSON_PROTOCOL.md) · [Constraint model](docs/specifications/CONSTRAINT_MODEL.md) ·
[Security](SECURITY.md).

## Contributing

Contributions are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) and [RULES.md](RULES.md) first, and follow the
[Code of Conduct](CODE_OF_CONDUCT.md). AI coding agents should also read [CLAUDE.md](CLAUDE.md) /
[AGENTS.md](AGENTS.md).

## License

[MIT](LICENSE)
