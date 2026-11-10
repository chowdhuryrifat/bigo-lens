# Component Design

This document covers each component of BigO Lens: its purpose, public API, inputs and outputs, status, the tests that cover it, and future work. The ownership rules and module contracts are in [SYSTEM_DESIGN.md](SYSTEM_DESIGN.md). Milestones are defined in [docs/development/ROADMAP.md](../development/ROADMAP.md).

Status key: **Implemented** means complete for its current scope. **Partial** means it works for a documented subset. **Planned (Mx)** means it does not exist yet.

| Component | Status | Section |
|---|---|---|
| Engine CLI | Implemented | [1](#1-engine-cli-bigolens-engine) |
| Protocol parsing | Implemented | [2](#2-protocol-parsing-apiprotocol) |
| Request handler | Implemented | [3](#3-request-handler-apirequesthandler) |
| Models | Implemented | [4](#4-models) |
| Symbolic expressions | Implemented | [5](#5-symbolic-expressions-complexityexpression) |
| Evaluator | Implemented | [6](#6-evaluator-complexityevaluator) |
| Asymptotic derivation | Implemented | [7](#7-asymptotic-derivation-complexityasymptotic) |
| Constraint set | Implemented | [8](#8-constraint-set-constraintsconstraint_set) |
| Workload evaluation | Partial | [9](#9-workload-evaluation-constraintsworkload) |
| Memory estimator | Partial | [10](#10-memory-estimator-estimationmemory_estimator) |
| STL cost model | Partial (data only) | [11](#11-stl-cost-model-estimationstl_cost_model) |
| Feasibility | Implemented (time input Planned M4) | [12](#12-feasibility) |
| Source analyzer interface | Implemented | [13](#13-source-analyzer-interface-analyzers) |
| Clang frontend | Partial | [14](#14-clang-frontend-parserclang_frontend) |
| Benchmark statistics and growth fit | Implemented | [15](#15-benchmark-statistics-and-growth-fit) |
| Benchmark runner stack | Planned (M4) | [16](#16-benchmark-runner-stack-planned-m4) |
| Shared schemas and examples | Implemented | [17](#17-shared-schemas-and-examples) |
| Extension components | Implemented (scaffold) | [18](#extension-components) |

---

## 1. Engine CLI (`bigolens-engine`)

- **Purpose:** the process entry point that speaks the JSON protocol.
- **Public API:** `bigolens-engine [--request FILE]`, `--version` (prints `bigolens-engine <version> (protocol 1.0)`), `--help` or `-h`.
- **Inputs:** one JSON request from stdin or `FILE`, read up to 4 MiB + 1 byte.
- **Outputs:** one JSON response line on stdout. Exit code 0 whenever a response was written (including error responses), 2 for a usage error, 1 otherwise.
- **Status:** Implemented. Source: `engine/src/main.cpp`, `engine/src/version.hpp.in`.
- **Tests:** the process-level behaviour is exercised by the extension integration test against the real engine, when one is available. `RequestHandler` behaviour is covered by `engine/tests/protocol_test.cpp`.
- **Future work:** a persistent mode is deliberately deferred ([ADR 0004](DECISIONS/0004-json-ipc.md)).

## 2. Protocol parsing (`api/protocol`)

- **Purpose:** strict validation of request JSON into `api::Request`.
- **Public API:** `parseRequest(const nlohmann::json&) -> ParseOutcome`. Types `Action`, `Request`, `ProtocolError`, `ParseOutcome`. Constants `kMaxRequestBytes`, `kMaxSourceBytes`.
- **Inputs:** a parsed JSON document.
- **Outputs:** an optional `Request`, the recovered `requestId`, and `ProtocolError`s with JSON-pointer paths.
- **Status:** Implemented.
- **Tests:** `engine/tests/protocol_test.cpp`: `UnknownFieldsAreRejected`, `IncompatibleSchemaVersionIsRejected`, `AnalyzeRequiresSource`, `InvalidRequestIdIsRejected`, `ConstraintValuesAreRangeChecked`, `IntegralFloatConstraintsAreAccepted`, `SumConstraintWithoutBoundsIsRejected`, `NonCppLanguageIsRejected`.
- **Future work:** new optional fields only in new minor protocol versions.

## 3. Request handler (`api::RequestHandler`)

- **Purpose:** the never-throw boundary. It dispatches actions and assembles responses.
- **Public API:** `RequestHandler(analyzers::SourceAnalyzer*)`, `handleText(std::string_view)`, `handle(const nlohmann::json&)`.
- **Inputs:** raw request text or JSON, and an optional analyzer.
- **Outputs:** a response JSON object (`schema_version`, `request_id`, `status`, `engine`, `results`, `diagnostics`, `errors`).
- **Status:** Implemented. The `capabilities` and `analyze` actions are implemented. `benchmark` returns `not_implemented`.
- **Tests:** `engine/tests/protocol_test.cpp`: `CapabilitiesSucceedWithoutFrontend`, `MalformedJsonIsAStructuredError`, `ExcessiveNestingIsRejectedBeforeParsing`, `OversizedRequestIsRejected`, `AnalyzeWithoutFrontendReportsUnavailable`, `BenchmarkIsNotImplemented`. `tests/integration/frontend_pipeline_test.cpp` covers the full analyze path.
- **Future work:** wire up the `benchmark` action (M4), and pass a runtime estimate to `assessTime` (M4).

## 4. Models

- **Purpose:** the shared result vocabulary.
- **Public API:** `BoundStatus`, `Confidence`, `FeasibilityStatus`, `Severity`, `SourceLocation`, `Diagnostic`, `weakest()`, `toString()` overloads; `Finding`, `FindingCategory`.
- **Inputs and outputs:** value types only.
- **Status:** Implemented (`engine/src/models/types.cpp`).
- **Tests:** covered indirectly through the protocol, workload, feasibility and integration tests.
- **Future work:** none planned beyond what new finding categories require.

## 5. Symbolic expressions (`complexity/expression`)

- **Purpose:** immutable, shareable cost formulas.
- **Public API:** `Expr`, `ExprKind`, `ExprPtr`; factories `constant`, `variable`, `add`, `subtract`, `multiply`, `power`, `logarithm`, `summation`, `maximum`; `isConstant`, `freeVariables`, `toString`.
- **Inputs:** operands and constants. Invalid input throws `std::invalid_argument`.
- **Outputs:** `ExprPtr`, simplified only where the simplification preserves meaning.
- **Status:** Implemented.
- **Tests:** `engine/tests/expression_test.cpp`: `ConstantFoldingInAddAndMultiply`, `IdentityElementsAreDropped`, `FreeVariablesExcludeSummationIndex`, `InvalidPowerAndLogarithmThrow`, `AllKindsRender`.
- **Future work:** further node kinds only when an analyzer needs them, for example recurrence terms ([ADR 0005](DECISIONS/0005-symbolic-expression-trees.md)).

## 6. Evaluator (`complexity/evaluator`)

- **Purpose:** numeric evaluation of an `Expr` under bindings.
- **Public API:** `evaluate(expr, Bindings, EvalOptions) -> EvalResult`; `EvalStatus { Ok, UnboundVariable, Overflow, DomainError, BudgetExceeded }`; `EvalOptions::budget` (default 50,000,000).
- **Inputs:** an expression and variable values (`double`).
- **Outputs:** a value or a failure status with `detail`.
- **Status:** Implemented.
- **Tests:** `engine/tests/evaluator_test.cpp`: `EvaluatesArithmetic`, `UnboundVariableIsReported`, `TriangularSummationMatchesClosedForm`, `EmptySummationIsZero`, `LogarithmClampsArgumentToOne`, `OverflowIsReportedNotReturned`, `NonAffineSummationRespectsBudget`, `MaximumPicksLargest`.
- **Future work:** exact integer or interval arithmetic for values above 2^53 (not scheduled).

## 7. Asymptotic derivation (`complexity/asymptotic`)

- **Purpose:** derive an O() upper bound from an `Expr`.
- **Public API:** `asymptoticUpperBound(expr) -> std::optional<AsymptoticBound>`, `toString(AsymptoticBound)`; `GrowthFactor`, `Monomial`.
- **Inputs:** an expression.
- **Outputs:** the non-dominated monomials, or `nullopt` for unsupported shapes.
- **Status:** Implemented.
- **Tests:** `engine/tests/asymptotic_test.cpp`: `Linear`, `Constant`, `Product`, `DominatedTermsAreDropped`, `TriangularSummationIsQuadratic`, `Logarithmic`, `Linearithmic`, `IncomparableTermsAreKept`.
- **Future work:** constraint-aware dominance, where for example `m <= n` would allow dropping terms.

## 8. Constraint set (`constraints/constraint_set`)

- **Purpose:** an in-memory model of the user's constraints.
- **Public API:** `ConstraintSet`, `VariableBound`, `SumConstraint`, `ProductConstraint`, `LinearConstraint`, `kMaxConstraintMagnitude`, `validate()`, `findSumConstraint()`.
- **Inputs:** populated by `parseRequest`.
- **Outputs:** a list of validation problems.
- **Status:** Implemented.
- **Tests:** `engine/tests/constraints_test.cpp`: `ValidSetHasNoProblems`, `InvertedRangeIsRejected`, `SumConstraintNeedsDeclaredBounds`, `ProductAndLinearMustReferenceDeclaredVariables`, `MagnitudeLimitIsEnforced`.
- **Future work:** see [CONSTRAINT_MODEL](../specifications/CONSTRAINT_MODEL.md).

## 9. Workload evaluation (`constraints/workload`)

- **Purpose:** compute a worst-case structural count under the constraints.
- **Public API:** `evaluateWorkload(expr, constraints) -> WorkloadResult`, `isMonotoneNonDecreasing`, `isSuperadditiveIn`.
- **Inputs:** an expression and a `ConstraintSet`.
- **Outputs:** `status`, `upperBound`, `method` (`pointwise-maxima`, `pointwise-maxima-capped`, `sum-constraint-packing` or `sum-constraint-cases`), `assumptions` and `missing`.
- **Status:** Partial. Product and linear constraints do not tighten bounds, and sum constraints over different count variables give `Unknown`.
- **Tests:** `engine/tests/workload_test.cpp`: `PointwiseMaximaForMonotoneExpression`, `MissingBoundGivesUnknown`, `SumConstraintPreventsTTimesNSquared`, `SumConstraintPackingWithSmallPerCaseMaximum`, `LinearPerCaseCostUnderSumConstraintIsTheTotal`, `NonMonotoneExpressionIsOnlyEstimated`, `SuperadditivityRecognition`. Integration: `SumConstrainedTestCasesUseConstraintRelationship`, `MissingConstraintsLeaveWorkloadUnknown`.
- **Future work (M3):** use product and linear constraints, and support multiple count variables.

## 10. Memory estimator (`estimation/memory_estimator`)

- **Purpose:** conservative static memory lower bounds.
- **Public API:** `estimateMemory(items, constraints) -> MemoryEstimate`, `StorageItem`, `StorageKind`, `toString(StorageKind)`.
- **Inputs:** storage items from the frontend, and the constraints.
- **Outputs:** static storage bytes (saturating, `Proven`), the largest local array, the estimated minimum and its status, and assumptions.
- **Status:** Partial. Only the element buffer of a sized `std::vector` is counted. Nested ownership is not followed, and no peak or upper bound is reported.
- **Tests:** `engine/tests/memory_estimator_test.cpp`: `StaticStorageIsSummedAndProven`, `StaticStorageSaturatesInsteadOfOverflowing`, `VectorBufferEvaluatedAtConstraintMaximum`, `UnboundedVectorLeavesMinimumUnknown`, `LargestLocalArrayIsTracked`. Integration: `LinearSpaceVectorAndStaticArray`, `SortingReportsVectorAndLinearInputLoop`.
- **Future work:** more container kinds, and measured peak memory from the benchmark (M4).

## 11. STL cost model (`estimation/stl_cost_model`)

- **Purpose:** standard-library operation costs (average and worst case, amortised flag).
- **Public API:** `stlCostTable()`, `lookupStlCost(container, operation)`, `StlOperationCost`.
- **Inputs and outputs:** a static table, exposed in the `capabilities` results as `stl_cost_model`.
- **Status:** Partial, data only. It is **not applied to findings**: library calls in a nest are listed as limitations and their cost is not included.
- **Tests:** `engine/tests/stl_cost_model_test.cpp`: `UnorderedMapDistinguishesAverageAndWorstCase`, `VectorPushBackIsAmortized`, `SortIsLinearithmic`, `RequiredOperationsArePresent`.
- **Future work:** apply the costs in the frontend when a call can be resolved to a table entry.

## 12. Feasibility

- **Purpose:** classify a time or memory risk against the limits.
- **Public API:** `assessTime`, `assessMemory`, `RuntimeEstimate`, `FeasibilityPolicy`, `FeasibilityAssessment`.
- **Inputs:** an optional runtime estimate, a `MemoryEstimate`, an optional known upper bound, and the limits.
- **Outputs:** a status and a reason.
- **Status:** the logic is Implemented. Time is always `Unknown` today because no runtime estimate exists (Planned M4). Memory can only be `HighRisk` or `Unknown` from static analysis.
- **Tests:** `engine/tests/feasibility_test.cpp`: `TimeUnknownWithoutEstimateOrLimit`, `TimeClassification`, `MemoryHighRiskFromStaticStorageAlone`, `MemoryNeverLikelySafeFromLowerBoundsOnly`, `MemoryWithKnownUpperBound`.
- **Future work:** feed it benchmark-derived runtime and peak-memory estimates ([FEASIBILITY_MODEL](../specifications/FEASIBILITY_MODEL.md)).

## 13. Source analyzer interface (`analyzers`)

- **Purpose:** keep the core independent of Clang.
- **Public API:** `SourceAnalyzer::analyze(const SourceInput&) -> SourceAnalysis`, `SourceAnalyzer::frontendVersion()`, `SourceInput`, `SourceAnalysis`.
- **Status:** Implemented.
- **Tests:** exercised through the Clang implementation in the integration tests. The null-analyzer path is covered by `AnalyzeWithoutFrontendReportsUnavailable`.
- **Future work:** an in-memory fake analyzer for unit tests of the analyze-path response assembly (not scheduled).

## 14. Clang frontend (`parser/clang_frontend`)

- **Purpose:** static analysis of C++ source with Clang LibTooling.
- **Public API:** `parser::makeClangSourceAnalyzer(ClangFrontendOptions{resourceDir})`. The internals (`ClangSourceAnalyzer`, `AnalysisAction`, `AnalysisConsumer`, `AnalysisVisitor`, `LoopAnalyzer`, `CollectingDiagnostics`) are private to the file.
- **Inputs:** a `SourceInput` (display path, content, C++ standard).
- **Outputs:** loop-nest, recursion, static-storage and dynamic-allocation findings; storage items; up to 50 error diagnostics.
- **Status:** Partial. The supported loop forms and status rules are in [ANALYSIS_MODEL](../design/ANALYSIS_MODEL.md). Interprocedural analysis, recurrence solving and the application of STL costs are not implemented.
- **Tests:** `tests/integration/frontend_pipeline_test.cpp` with `tests/fixtures/cpp/`: `LinearLoop` (01), `NestedLoops` (02), `TriangularLoopHasExactStructuralCount` (03), `LogarithmicLoop` (04), `SortingReportsVectorAndLinearInputLoop` (05), `BinarySearchWhileLoopIsUnknownNotGuessed` (06), `NaiveFibonacciRecursionIsUnsupported` (07), `LinearSpaceVectorAndStaticArray` (08), `SumConstrainedTestCasesUseConstraintRelationship` (09), `DataDependentTerminationIsUnknown` (10), `SequentialInnerLoopsAreSummed`, `ModifiedIndexIsUnsupported`, `InvalidSourceReportsDiagnosticsAndNoFindings`, `FrontendVersionIsReported`.
- **Future work (M2):** more loop shapes, an interface for solving recurrences, interprocedural cost propagation, and application of the STL cost model.

## 15. Benchmark statistics and growth fit

- **Purpose:** turn measured samples into robust summaries and growth-model fits.
- **Public API:** `summarize`, `median`, `SampleSummary`; `fitGrowth`, `predict`, `GrowthModel`, `Measurement`, `ModelFit`, `GrowthFitResult`, `GrowthFitOptions`, `toString(GrowthModel)`.
- **Inputs:** runtime samples (ms) and `(n, ms)` points.
- **Outputs:** a summary, a list of fits ordered best first, an optional selected model, and an ambiguity flag.
- **Status:** Implemented. Nothing calls it yet.
- **Tests:** `benchmark/tests/statistics_test.cpp`: `OddCountMedianAndMad`, `EvenCountMedianIsMidpoint`, `OutlierDoesNotMoveMedian`, `RejectsInvalidSamples`. `benchmark/tests/growth_fit_test.cpp`: `SelectsQuadraticForQuadraticData`, `SelectsLinearForLinearData`, `SelectsLinearithmicForNLogNData`, `ConstantDataIsAmbiguous`, `RejectsTooFewOrInvalidPoints`.
- **Future work:** more models (for example log n and n^3), and confidence intervals feeding `RuntimeEstimate`.

## 16. Benchmark runner stack (Planned M4)

- **Purpose:** compile and run user code under controlled conditions to measure runtime and memory.
- **Planned parts:** compiler adapter (fixed argv, no shell), input generators, runner (timeouts, memory limits, process-tree cleanup, temporary directory), resource monitor, and platform isolation (Linux rlimits and namespaces or seccomp where available; Windows Job Objects; on macOS, `sandbox-exec` is deprecated).
- **Status:** Not implemented. The `benchmark` action returns `not_implemented`.
- **Tests:** none yet. `BenchmarkIsNotImplemented` asserts the current behaviour.
- **Future work:** see [BENCHMARK_SPECIFICATION](../specifications/BENCHMARK_SPECIFICATION.md) and [THREAT_MODEL](../engineering/THREAT_MODEL.md). Until isolation exists, benchmarking must be documented as not sandboxed.

## 17. Shared schemas and examples

- **Purpose:** the single source of truth for the protocol contract.
- **Public API:** `shared/schemas/request.schema.json`, `shared/schemas/response.schema.json` (JSON Schema draft 2020-12); example requests in `shared/examples/` (`capabilities`, `analyze`, `benchmark`, `invalid` and `malformed.request.txt`) and the matching `*.response.json` files, which `scripts/development/regenerate-examples.sh` produces by running the real engine.
- **Status:** Implemented.
- **Tests:** schema validation in the CI extension job; response validation in the extension unit and integration tests.
- **Future work:** regenerate the example responses in CI and fail if they drift from the committed copies.

## Extension components

Part of the TypeScript VS Code extension in `extension/src/`. Status: Implemented (scaffold).

| Component | Location | Public surface | Inputs and outputs | Tests |
|---|---|---|---|---|
| Commands | `extension/src/commands/` | `bigoLens.analyzeActiveFile` ("BigO Lens: Analyze Active C++ File"), `bigoLens.editConstraints` ("BigO Lens: Edit Constraints for Active File"), `bigoLens.showCapabilities` ("BigO Lens: Show Engine Capabilities") | Input: the active editor. Output: updates to the UI. | VS Code extension-host smoke test (activation and command registration) through `@vscode/test-electron`, under xvfb in CI |
| Engine locator | `extension/src/services/engineLocator.ts` | Resolves `bigoLens.enginePath`, then `BIGOLENS_ENGINE_PATH`, then the bundled `bin/`, then the dev build | Input: settings and environment. Output: a path or an error. | Unit test (node:test) |
| Constraints file | `extension/src/services/constraintsStore.ts` | Reads, validates (ajv) and templates `<file>.bigolens.json` | Input: the sidecar JSON. Output: a `constraints` object or validation errors. | Unit test |
| Request builder | `extension/src/services/requestBuilder.ts` | Builds a protocol 1.0 request | Input: document, constraints and `bigoLens.cppStandard`. Output: request JSON. | Unit test |
| Engine client | `extension/src/services/engineClient.ts` | Spawns the engine (`shell: false`, no arguments), sends the request on stdin, caps stdout at 16 MiB, enforces `bigoLens.analysisTimeoutMs`, validates the response | Input: request and engine path. Output: a validated response or a failure. | Unit tests against a fake engine script; integration test against the real engine (skipped with a message when no engine is available) |
| Response validation | `extension/src/services/schemaValidator.ts` | ajv validation against `extension/schemas/response.schema.json` | Input: untrusted JSON. Output: typed data or errors. | Unit test |
| Result providers | `extension/src/providers/` | "Analysis Results" tree view (pattern, O(), workload, bound status, confidence, memory and feasibility summary, navigation to source), Information-level diagnostics in the Problems panel, the "BigO Lens" output channel | Input: a validated response. Output: the UI. | Smoke test |
| Models | `extension/src/models/` | TypeScript types for the protocol | none | Compiled with `tsc` |

**Settings:** `bigoLens.enginePath` (string, default `""`), `bigoLens.cppStandard` (`c++11` to `c++23`, default `c++20`), `bigoLens.analysisTimeoutMs` (default 30000). See [CONFIGURATION](../user/CONFIGURATION.md).

**Future work (M5 or later):** a webview report (strict CSP, nonce scripts), charts, benchmark controls and problem-statement extraction. See [UI_UX_DESIGN](../design/UI_UX_DESIGN.md).
