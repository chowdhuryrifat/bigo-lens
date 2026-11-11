# System Design

This document describes the implementation-level design of BigO Lens 0.1.0 (protocol 1.0): the classes and functions, the data structures, who owns what, and the contract each module provides. For the overall picture, read [ARCHITECTURE.md](../../ARCHITECTURE.md) first. For per-component status and test coverage, see [COMPONENT_DESIGN.md](COMPONENT_DESIGN.md).

Status labels follow [ARCHITECTURE.md](../../ARCHITECTURE.md#bigo-lens-architecture): Implemented, Partial or Planned (Mx).

As approved in [ADR 0012](DECISIONS/0012-rust-engine-architecture.md), the engine is architected in Rust across three crates in `crates/`: `bigolens-core` (pure Rust, `#![forbid(unsafe_code)]`), `bigolens-clang` (FFI boundary to Clang LibTooling), and `bigolens-engine` (CLI application). During the phased migration, the C++ implementation under `engine/` serves as the baseline reference.

---

## 1. Cross-cutting design principles

| Principle | How it is realised |
|---|---|
| Honest results over complete results | Every quantity carries a `BoundStatus` and `Confidence`. When the engine cannot prove something, the answer is `Unknown` or `Unsupported`, never a guess ([ADR 0008](DECISIONS/0008-conservative-unknown-unsupported-results.md)). |
| Clang isolation | Only `engine/src/parser/clang_frontend.cpp` includes Clang. Everything else depends on `analyzers::SourceAnalyzer`. |
| Value semantics | Models are plain structs. Expressions are immutable and shared. |
| No exceptions across the process boundary | `api::RequestHandler` turns every failure into a JSON error response. |
| Bounded resources | Request, source, JSON-depth, diagnostic-count and evaluation-budget limits are all compile-time constants or defaults. |
| Deterministic output | Finding IDs `F1..Fn` follow source order. Expressions render canonically. Outputs contain no timestamps or randomness. |

---

## 2. Ownership model

| Object | Owner | Lifetime | Sharing |
|---|---|---|---|
| `analyzers::SourceAnalyzer` (concrete `ClangSourceAnalyzer`) | `main()` through `std::unique_ptr<analyzers::SourceAnalyzer>` from `parser::makeClangSourceAnalyzer()` | The whole process | Not copyable or movable (copy and move operations are deleted) |
| `api::RequestHandler` | Stack object in `main()` | The whole process | Holds a **non-owning** `analyzers::SourceAnalyzer*`. The pointer may be `nullptr` when the engine is built without Clang. The handler must not outlive the analyzer. |
| `complexity::Expr` nodes | Reference-counted through `ExprPtr = std::shared_ptr<const Expr>` | Until the last reference is dropped | Immutable, so subtrees are shared freely between findings, storage items and cost-table entries. It is safe to read them from multiple threads. |
| `models::Finding`, `estimation::StorageItem` | `analyzers::SourceAnalysis` (by value in vectors) | One request | Their expression fields are `ExprPtr`. A null pointer means "no expression". |
| `constraints::ConstraintSet` | `api::Request` (by value) | One request | Passed by const reference to the workload, memory and feasibility code |
| STL cost table | Function-local static returned by `estimation::stlCostTable()` | The whole process | Read-only |
| JSON documents | `nlohmann::json` values | One request | Created only in `api` |

No module keeps mutable global state. The engine handles one request per process, but `RequestHandler::handle` is `const` and does not depend on any earlier request.

---

## 3. Module designs

### 3.1 `api`: protocol and request handling (Implemented)

Headers: `api/protocol.hpp`, `api/request_handler.hpp`.

**Data structures**

```cpp
inline constexpr std::size_t kMaxRequestBytes = 4U * 1024U * 1024U;  // 4 MiB
inline constexpr std::size_t kMaxSourceBytes  = 2U * 1024U * 1024U;  // 2 MiB
enum class Action { Capabilities, Analyze, Benchmark };
struct ProtocolError { std::string code, message, path; };           // path = JSON pointer
struct Request { schemaVersion, requestId, action, sourcePath, sourceContent,
                 constraints::ConstraintSet constraints, cppStandard = "c++20"; };
struct ParseOutcome { std::optional<Request> request;
                      std::optional<std::string> requestId;          // recovered even if invalid
                      std::vector<ProtocolError> errors; };
```

**Functions and classes**

- `ParseOutcome parseRequest(const nlohmann::json&)` is a pure function. Unknown fields are rejected. Integer magnitudes must be at most 1e18, and integral floats such as `2e5` are accepted. Variable names must be C++ identifiers of at most 64 characters, optionally suffixed with `.size()`. `memory_limit_mb` is converted to `ConstraintSet::memoryLimitBytes`. `request_id` must match `[A-Za-z0-9_.:-]{1,128}`. `schema_version` must have the same major version as the engine and a minor version no greater than the engine's.
- `class RequestHandler`
  - `explicit RequestHandler(analyzers::SourceAnalyzer* analyzer) noexcept` takes a non-owning pointer, which may be null.
  - `nlohmann::json handleText(std::string_view) const` checks the size limit, then the nesting depth (64, checked by a linear scan before the recursive parser runs), then parses with `allow_exceptions=false`, then delegates to `handle`.
  - `nlohmann::json handle(const nlohmann::json&) const` validates, dispatches and assembles the response.

**Contract**

| | |
|---|---|
| Preconditions | None. Any byte string is acceptable input to `handleText`. |
| Postconditions | Returns exactly one JSON object that conforms to `response.schema.json`. `status == "ok"` implies `errors` is empty and `results` is non-null. `status == "error"` implies `results` is null and `errors` is non-empty. The `engine` block is always present, and `frontend` is the Clang version string or null. |
| Never-throw boundary | Any `std::exception` raised during dispatch or analysis is caught and converted to `internal_error`, with a note on stderr. This is the last point where exceptions may travel. Nothing past it can see a C++ exception. |
| Dependencies | `analyzers`, `complexity`, `constraints`, `estimation`, `feasibility`, `models`, the generated `version.hpp` (`kEngineName`, `kEngineVersion`, `kProtocolVersion`), and nlohmann/json. |

### 3.2 `models`: shared vocabulary (Implemented)

Headers: `models/types.hpp`, `models/finding.hpp`.

- `enum class BoundStatus { Proven, Estimated, Unknown, Unsupported }` with `weakest(a, b)` for combining statuses. The weakness order is Proven, then Estimated, then Unknown, then Unsupported.
- `enum class Confidence { High, Medium, Low, None }`, `enum class FeasibilityStatus { LikelySafe, Borderline, HighRisk, Unknown }`, `enum class Severity { Error, Warning, Information }`.
- `struct SourceLocation { file; line; column; optional endLine; optional endColumn; }`, `struct Diagnostic { severity; message; optional<SourceLocation> location; }`.
- `enum class FindingCategory { LoopNest, Recursion, StaticStorage, DynamicAllocation, UnsupportedConstruct }`.
- `struct Finding { id; category; pattern; description; functionName; location; ExprPtr timeExpression; ExprPtr spaceExpression; boundStatus; confidence; assumptions; limitations; provenance; }`.
- `toString(...)` overloads give the protocol spelling, for example `BoundStatus::Proven` becomes `"proven"` and `FindingCategory::LoopNest` becomes `"loop_nest"`.

Contract: these are value types with no invariants beyond what the enumerations allow. A null `timeExpression` means "no time quantity" and is serialised as `null`.

### 3.3 `complexity`: symbolic expressions (Implemented)

Headers: `complexity/expression.hpp`, `complexity/evaluator.hpp`, `complexity/asymptotic.hpp`.

**Expression tree**

- `enum class ExprKind { Constant, Variable, Add, Multiply, Power, Logarithm, Summation, Maximum }`.
- `class Expr` is immutable. Its accessors are `kind()`, `number()` (a constant value, an exponent or a logarithm base), `name()` (a variable name or summation index) and `operands()`. `using ExprPtr = std::shared_ptr<const Expr>`.
- Factories: `constant`, `variable`, `add`, `subtract`, `multiply`, `power(base, exponent)`, `logarithm(argument, base = 2.0)`, `summation(index, lower, upper, body)` and `maximum`. They apply **only semantics-preserving simplification**: flattening, constant folding and dropping identity elements. For example, `multiply` does not merge `n * n` into `n^2`. Invalid construction throws: a null operand, a non-finite constant, an empty variable or index name, a negative or non-finite exponent, a logarithm base that is not a finite constant greater than 1, or a `maximum` with no operands. These exceptions stay inside the engine and are caught by `RequestHandler`.
- `Logarithm` evaluates `log_base(max(x, 1))`.
- Helpers: `isConstant(expr, value)`, `freeVariables(expr)` (summation indices are excluded) and `toString(expr)`. `toString` produces the canonical human-readable rendering, for example `sum_{i=0}^{n - 1} i`.

**Evaluator**

- `EvalResult evaluate(const Expr&, const Bindings&, const EvalOptions& = {})`, where `Bindings = std::map<std::string, double, std::less<>>`.
- `EvalStatus { Ok, UnboundVariable, Overflow, DomainError, BudgetExceeded }`. A non-finite intermediate result produces `Overflow`. Summations with an affine body use closed forms. Other summations are iterated within `EvalOptions::budget`, which defaults to 50,000,000 node evaluations.
- IEEE `double` arithmetic is used throughout. Values above 2^53 are approximate.
- Contract: `evaluate` never throws for numeric problems. When `status != Ok`, `value` is meaningless and `detail` explains why.

**Asymptotic derivation**

- `std::optional<AsymptoticBound> asymptoticUpperBound(const Expr&)`. An `AsymptoticBound` is a list of `Monomial`s (`std::map<variable, GrowthFactor{power, logPower}>`).
- Assumptions: every variable grows without bound and is at least 1. Dominated monomials are removed, but incomparable ones are kept (for example `O(m n + n^2)`). Terms with negative coefficients are dropped, which keeps an upper bound. The function returns `nullopt` for shapes it does not support, and the response then contains `"asymptotic": null`.
- `toString(const AsymptoticBound&)` renders `O(...)`, with variables ordered by name inside each monomial.

### 3.4 `constraints`: constraint model and workload (Partial)

Headers: `constraints/constraint_set.hpp`, `constraints/workload.hpp`.

**Data structures**

```cpp
inline constexpr std::int64_t kMaxConstraintMagnitude = 1e18;
struct VariableBound   { std::optional<int64_t> min, max; };
struct SumConstraint   { std::string variable, countVariable; int64_t maxTotal; };  // sum of variable over countVariable cases <= maxTotal
struct ProductConstraint { std::vector<std::string> variables; int64_t maxProduct; };
struct LinearConstraint  { std::map<std::string, int64_t> coefficients; int64_t bound; };
struct ConstraintSet { std::map<std::string, VariableBound, std::less<>> variables;
                       sums; products; linear; optional timeLimitMs; optional memoryLimitBytes; };
```

**Functions**

- `std::vector<std::string> validate(const ConstraintSet&)` checks for inverted ranges, magnitudes above 10^18, sum constraints whose variable or count variable lacks a declared upper bound (or that coincide, or repeat a variable), and product or linear constraints that reference undeclared variables. The function returns human-readable problems, and an empty vector means the set is valid.
- `const SumConstraint* findSumConstraint(cs, variable)` returns a non-owning pointer into `cs`, or null.
- `WorkloadResult evaluateWorkload(const Expr&, const ConstraintSet&)` returns `{status, optional upperBound, method, assumptions, missing}`.
- `bool isMonotoneNonDecreasing(const Expr&)` is a syntactic check and therefore conservative: it can say false for an expression that is in fact monotone.
- `bool isSuperadditiveIn(const Expr&, variable)` recognises shapes such as `n`, `n^2`, `n log n`, `n(n-1)/2`-style summations, and products with monotone factors.

**Workload contract**

| Condition | Result |
|---|---|
| A free variable has no `max` | `Unknown`, with `missing` listing the variables |
| Monotone, no negative minima, no sum constraint | `Proven`, method `pointwise-maxima` |
| Not provably monotone | `Estimated`, with an assumption saying so |
| One sum constraint, separable `t * perCase` or a per-case cost, superadditive | method `sum-constraint-packing`: `floor(S / n_max)` full cases plus one remainder case, capped at `t_max` cases |
| Sum constraint, not superadditive | method `sum-constraint-cases`: cases times the per-case maximum, which is still sound |
| Count variable not separable | method `pointwise-maxima-capped` |
| Sum constraints over different count variables | `Unknown` |
| Evaluation failure (overflow, budget) | `Unknown`, with an assumption `"evaluation failed: ..."` |
| Product or linear constraints present | Recorded as assumptions only. They do not tighten bounds yet. Planned for M3. |

Postcondition: `upperBound` is present if and only if `status` is `Proven` or `Estimated`. A `Proven` bound really is an upper bound on the structural count under the stated assumptions.

### 3.5 `estimation`: memory and STL costs (Partial)

Headers: `estimation/memory_estimator.hpp`, `estimation/stl_cost_model.hpp`.

- `enum class StorageKind { GlobalStatic, LocalArray, VectorBuffer }`. `struct StorageItem { name; kind; typeName; optional<uint64_t> exactBytes; ExprPtr symbolicBytes; SourceLocation location; }`.
- `MemoryEstimate estimateMemory(items, constraints)` returns:
  - `staticStorageBytes`: a saturating `uint64_t` sum of `GlobalStatic` items, with `staticStorageSaturated` set on overflow. This is a proven lower bound.
  - `largestLocalArrayBytes`.
  - `estimatedMinimumBytes` and `estimatedMinimumStatus`. The value is static storage plus every `VectorBuffer` evaluated with `evaluateWorkload`, and the status is `Estimated`. With no dynamic items it equals static storage and the status is `Proven`. It is absent if any vector size cannot be evaluated.
  - `assumptions`, which always state that the baseline, stack, library internals and unmodelled heap allocations are excluded.
  - Contract: the function **never** reports an upper bound or peak memory.
- `struct StlOperationCost { container; operation; ExprPtr average; ExprPtr worst; bool amortized; note; }`. `stlCostTable()` returns the static table, and `lookupStlCost(container, operation)` returns `std::optional<StlOperationCost>`. The table is exposed through `capabilities` but is **not yet applied to findings**. Applying it is planned.

### 3.6 `feasibility` (Implemented; time input Planned M4)

Header: `feasibility/feasibility.hpp`.

- `struct RuntimeEstimate { lowMs, centralMs, highMs; }`, `struct FeasibilityPolicy { timeSafeFraction = 0.5; memorySafeFraction = 0.8; }`, `struct FeasibilityAssessment { status; reason; }`.
- `assessTime(optional<RuntimeEstimate>, optional<int64_t> timeLimitMs, policy)`:
  - `Unknown` when there is no limit or no estimate.
  - `HighRisk` when `lowMs > limit`.
  - `LikelySafe` when `highMs <= 0.5 * limit`.
  - Otherwise `Borderline`.
  - `RequestHandler` passes `std::nullopt` because benchmarking does not exist yet, so **time feasibility is always `Unknown` today**.
- `assessMemory(MemoryEstimate, optional<double> knownUpperBoundBytes, optional<int64_t> memoryLimitBytes, policy)`:
  - `HighRisk` if static storage or the estimated minimum exceeds the limit.
  - `LikelySafe` or `Borderline` only with a known upper bound (at most 80% or at most 100% of the limit).
  - Otherwise `Unknown`. Static analysis passes no upper bound.

### 3.7 `analyzers` and `parser`: the frontend boundary (Partial)

Headers: `analyzers/source_analyzer.hpp`, `parser/clang_frontend.hpp`.

```cpp
struct SourceInput    { std::string displayPath, content; std::string cppStandard = "c++20"; };
struct SourceAnalysis { bool parsed = false; std::vector<models::Finding> findings;
                        std::vector<estimation::StorageItem> storage;
                        std::vector<models::Diagnostic> diagnostics; };
class SourceAnalyzer {           // non-copyable, non-movable, virtual destructor
  virtual SourceAnalysis analyze(const SourceInput&) = 0;
  virtual std::string frontendVersion() const = 0;
};
struct ClangFrontendOptions { std::string resourceDir; };
std::unique_ptr<analyzers::SourceAnalyzer> makeClangSourceAnalyzer(ClangFrontendOptions = {});
```

**Contract**

| | |
|---|---|
| Preconditions | `content` is at most 2 MiB (enforced by `parseRequest`). `cppStandard` is one of `c++11`, `c++14`, `c++17`, `c++20` or `c++23`. |
| Postconditions | If `parsed == false`, then `findings` and `storage` are empty and `diagnostics` holds at most 50 errors. If `parsed == true`, findings are in source order with IDs `F1..Fn`. Every finding has a `provenance` of `clang-loop-analyzer`, `clang-recursion-detector` or `clang-storage-analyzer`. |
| Side effects | The source is parsed in memory. `#include`d headers are read from disk, as a compiler would read them. Nothing is executed and nothing is written. |

**Internal structure of `clang_frontend.cpp`** (these types are private to the file):

- `CollectingDiagnostics` is a `clang::DiagnosticConsumer`. It collects up to 50 error diagnostics and suppresses warnings.
- `ClangSourceAnalyzer` is the `SourceAnalyzer` implementation. It runs the tool with `-fsyntax-only`, a fixed argument list and `-std=<cppStandard>` on an in-memory file.
- `AnalysisAction` and `AnalysisConsumer` connect the frontend action and the AST consumer.
- `AnalysisVisitor` is a `clang::RecursiveASTVisitor`. It visits function definitions in the main file (template primaries only), finds top-level loop nests, direct recursion and storage, and assigns finding IDs.
- `LoopAnalyzer` derives trip counts, builds the structural-count `Expr` and assigns `BoundStatus`, assumptions and limitations. Its rules:
  - If the loop index is written in the body, the result is `Unsupported`.
  - If a bound variable may be written in the body, the result is `Estimated`.
  - User-defined calls make the result `Estimated`, and the callee's cost is not included.
  - Library calls are added to limitations.
  - An early exit is still treated as an upper bound, with an assumption recorded.
  - A `do-while` loop is `Unsupported`.
  - Any other form is `Unknown`, described as "data-dependent termination".
- Confidence mapping: `Proven` with no limitations and no early exit is `High`. Any other `Proven` is `Medium`. `Estimated` is `Low`. `Unknown` and `Unsupported` are `None`.

The supported loop forms are listed in [docs/design/ANALYSIS_MODEL.md](../design/ANALYSIS_MODEL.md) and [docs/specifications/COMPLEXITY_MODEL.md](../specifications/COMPLEXITY_MODEL.md).

### 3.8 `benchmark` library (Partial; runner Planned M4)

Headers: `benchmark/include/bigolens/benchmark/statistics.hpp`, `growth_fit.hpp`. Namespace `bigolens::benchmark`.

- `std::optional<SampleSummary> summarize(std::vector<double> samples)` returns `{count, minimum, median, maximum, mad, relativeSpread}`. It returns `nullopt` if the input is empty or any sample is negative or non-finite. Measurements are never repaired. `double median(std::vector<double>&)` reorders its argument.
- `enum class GrowthModel { Linear, Linearithmic, Quadratic }`, `struct Measurement { n; milliseconds; }`, `struct ModelFit { model; intercept; slope; rss; r2; }`, `struct GrowthFitResult { fits (best first); optional selected; bool ambiguous = true; }`, `struct GrowthFitOptions { minimumPoints = 4; discriminationRatio = 0.5; }`.
- `std::optional<GrowthFitResult> fitGrowth(points, options)` fits `t = a + b * f(n)` by least squares, with f(n) equal to n, n log2 n or n squared. It returns `nullopt` for fewer than 4 distinct positive sizes or non-finite values. A model is selected only when the best RSS is less than 0.5 times the runner-up's and the slope is positive.
- `double predict(const ModelFit&, double n)`. The caller must label any extrapolation beyond the measured range.
- Contract: the library performs no I/O, compilation or process execution. It does not depend on the engine.
- Planned (M4): compiler adapter, runner, input generators, resource monitor and the protocol wiring for the `benchmark` action. See [BENCHMARK_SPECIFICATION](../specifications/BENCHMARK_SPECIFICATION.md).

### 3.9 Executable `bigolens-engine` (Implemented)

`engine/src/main.cpp`:

1. Parses the arguments. Unknown arguments produce usage on stderr and exit code 2.
2. Reads at most `kMaxRequestBytes + 1` bytes with `readBounded`, so oversized input is detected without buffering all of it.
3. Constructs the analyzer when `BIGOLENS_HAS_CLANG` is defined and uses null otherwise.
4. Calls `RequestHandler::handleText`, writes `dump()` and a newline, and returns 0 if the stream is good.

An unopenable `--request` file or a fatal exception produces exit code 1 with a message on stderr.

---

## 4. Extension design (Implemented scaffold)

The extension is in TypeScript (Node 22 tooling, VS Code ^1.85). Its source is organised under `extension/src/{commands,services,providers,models}`. The responsibilities below are the module contracts. The file-level API is described in [COMPONENT_DESIGN.md](COMPONENT_DESIGN.md#extension-components).

| Layer | Responsibility | Contract |
|---|---|---|
| `models/` | TypeScript types that mirror the request and response schemas | Types only, no behaviour |
| `services/` engine locator | Resolves the engine binary in this order: the `bigoLens.enginePath` setting, the `BIGOLENS_ENGINE_PATH` environment variable, the bundled `<extension>/bin/bigolens-engine[.exe]`, then a dev build at `<repo>/build/<dev\|release\|ci\|no-clang>/engine/[Debug\|Release/]bigolens-engine[.exe]` | Returns an existing file path or a clear "not found" error listing the places it checked |
| `services/` constraints file | Reads `<source file>.bigolens.json`, which holds the request's `constraints` object, and validates it with ajv against the request schema | Invalid files are reported and never sent to the engine |
| `services/` request builder | Builds a protocol 1.0 request from the document text, path, sidecar constraints and the `bigoLens.cppStandard` setting | The output validates against `request.schema.json` |
| `services/` engine client | Uses `child_process.spawn` with `shell: false` and no arguments, writes the request to stdin, caps stdout at 16 MiB, kills the process after `bigoLens.analysisTimeoutMs`, and validates the response with ajv against `response.schema.json` | Either resolves with a schema-valid response or rejects with a typed failure. It never passes unvalidated data on. |
| `providers/` | The "Analysis Results" tree view in the "BigO Lens" activity-bar container, Information-level diagnostics in the Problems panel for each finding, and the "BigO Lens" output channel | Consumes validated responses only |
| `commands/` | `bigoLens.analyzeActiveFile`, `bigoLens.editConstraints` (creates the sidecar from a template) and `bigoLens.showCapabilities` | Each command coordinates services and providers and contains no protocol logic of its own |

Planned (M5 or later): a webview report with a strict CSP and nonce scripts, charts, benchmark controls and problem-statement extraction.

---

## 5. Dependency matrix (engine)

Rows depend on the columns marked with an X.

| | models | complexity | constraints | estimation | feasibility | analyzers | api | Clang | nlohmann |
|---|---|---|---|---|---|---|---|---|---|
| models | | X | | | | | | | |
| complexity | | | | | | | | | |
| constraints | X | X | | | | | | | |
| estimation | X | X | X | | | | | | |
| feasibility | X | | | X | | | | | |
| analyzers | X | | | X | | | | | |
| api | X | X | X | X | X | X | | | X |
| parser | X | X | | X | | X | | X | |
| main | | | | | | X | X | (only if Clang enabled) | X |

`complexity` is the leaf module. Adding a dependency that creates a cycle, or that makes the core depend on Clang, violates [RULES.md](../../RULES.md).
