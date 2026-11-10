# Engineering Standards

This document expands the rules in [RULES.md](../../RULES.md) with the reasoning behind them and the standards reviewers apply. RULES.md says *what* is required. This document says *why*, and how to judge the cases the rules do not cover.

Related: [CODING_STANDARDS.md](CODING_STANDARDS.md), [TESTING_STRATEGY.md](TESTING_STRATEGY.md), [THREAT_MODEL.md](THREAT_MODEL.md), [PERFORMANCE.md](PERFORMANCE.md).

## 1. Maintainability

### 1.1 Module boundaries

The engine is split so that the parts that are hard to build are as small as possible:

| Layer | Target | May depend on | Must not depend on |
| --- | --- | --- | --- |
| Protocol, models, complexity, constraints, estimation, feasibility | `bigolens_core` | C++ std, nlohmann/json | Clang, LLVM, OS process APIs |
| Static analysis front end | `bigolens_clang_frontend` | `bigolens_core`, Clang LibTooling | VS Code, benchmark |
| Measurement statistics and growth fitting | `bigolens_benchmark` | C++ std | Clang, `bigolens_core` internals |
| CLI | `bigolens-engine` | all of the above | none |
| VS Code extension | `extension/` | engine via JSON protocol only | engine internals, C++ build |

Reviewers check these points:

- New logic that does not need the AST belongs in `bigolens_core`, where it can be unit-tested without Clang on every platform.
- The front end turns AST shapes into `models::Finding` and `complexity::Expr` values. It must not evaluate constraints, assess feasibility or serialize JSON. Those belong to the core.
- The extension does not re-implement analysis. When the extension needs information it cannot get, extend the protocol.

### 1.2 Design for change

- Prefer small, pure functions that take data and return values over classes with mutable state. Most of the core (`evaluateWorkload`, `asymptoticUpperBound`, `assessTime`, `assessMemory`) follows this pattern. Keep it that way.
- Use interfaces at real seams, where an alternative implementation exists or is planned: `SourceAnalyzer` (Clang front end versus none), the future recurrence solver, and the future compiler adapter. Do not add interfaces speculatively.
- Keep analysis decisions data-driven where it is practical. The STL cost model is a table. Feature maturity is reported through `capabilities.features`. When a capability's maturity changes, update that map in the same change.
- Functions longer than about 60 lines or nested deeper than 3 levels usually need splitting. `clang-tidy` flags cognitive complexity. Treat its warnings as review prompts.

### 1.3 Readability

- Name things after domain concepts from the specifications: *workload*, *bound status*, *finding*, *sum constraint*. Do not use implementation vocabulary such as *result2* or *helperData*.
- Put units in names when they are ambiguous: `timeLimitMs`, `memoryLimitMb`, `staticStorageBytes`.
- Literal constants that encode policy (thresholds, limits, budgets) are named `kConstants` at namespace scope and have a comment pointing to the specification that defines them.

## 2. Quality

### 2.1 Correctness over coverage of syntax

The analyzer is *partial by design*. Being right about fewer programs is better than being plausible about many. When you add support for a new construct:

1. Specify it first: the shape, the trip-count formula, the status rule and the confidence rule, in [COMPLEXITY_MODEL.md](../specifications/COMPLEXITY_MODEL.md).
2. Add positive fixtures plus at least one **near-miss** negative fixture. The near miss is a program that looks similar but must not match, for example a loop whose index is modified in the body.
3. Implement it.
4. Confirm that existing fixtures did not change classification. If one did, explain why in the pull request.

### 2.2 Soundness conventions

- A `proven` time bound is an upper bound on the structural count under the stated assumptions. If any step of the derivation is heuristic, the status is `estimated` or weaker.
- Early exits (`break`, `return`, `goto`, `throw`) keep the bound as an upper bound and add an assumption. They never tighten it.
- Calls to user-defined functions demote the status to `estimated`, and the callee cost is **not** included. Say so in `limitations`.
- Library calls are listed in `limitations`. Their cost is not included until the STL cost model is applied (planned).
- Workload evaluation must be sound for the reported status. If an argument requires monotonicity or superadditivity, check it syntactically and conservatively, and fall back when the check fails.
- Memory figures are lower bounds (static storage) or estimates (sized vectors at maxima). Never present them as peak usage.

### 2.3 Review standards

Reviewers look for these, in priority order:

1. Honesty of results: does any path report more certainty than it has?
2. Security: is untrusted input validated, and is there no shell or flag injection?
3. Protocol compatibility: are the schemas, types, docs and examples updated together?
4. Tests: is there a regression test, a near-miss test and a negative test?
5. Design: does the code respect layer boundaries and ownership rules?
6. Style: mostly automated, and the least important item to argue about.

## 3. Performance

Performance goals and measurement are in [PERFORMANCE.md](PERFORMANCE.md). The engineering standards that follow from them:

- **Bounded work for untrusted input.** Every loop over user-controlled data has a bound: request size, source size, nesting depth, diagnostic count, the evaluator budget (default 5×10^7 node evaluations for non-affine summations), and stdout size in the extension. A new algorithm whose cost depends on user input must state its bound.
- **Prefer closed forms to evaluation.** Affine summations are evaluated in closed form. Only non-affine summations fall back to iteration under the budget.
- **Parse once.** One translation unit per request. Do not re-parse to answer follow-up questions within a request.
- **Do not optimize without measurement.** Engine time is dominated by Clang parsing the included headers (for example `<iostream>` or `<bits/stdc++.h>`), not by BigO Lens's own analysis. Profile before you micro-optimize the analysis passes.
- Performance claims in pull requests must include the machine, the build type and the method used. Do not put performance thresholds in ordinary CI (see [TESTING_STRATEGY.md](TESTING_STRATEGY.md)).

## 4. Security

The threat model is in [THREAT_MODEL.md](THREAT_MODEL.md). The standards:

- **Defence in depth on both sides of the protocol.** The engine validates requests, and the extension validates responses. Neither trusts the other's output, so a compromised or buggy engine cannot drive the extension into misbehaviour.
- **No code execution without isolation.** The current engine only parses. Code that compiles or runs user programs (M4) is blocked from merging until the isolation requirements in the threat model are implemented for that platform, or the feature is clearly gated and labelled "not sandboxed".
- **Fixed compiler argv.** The only request-derived compiler input is the validated `cpp_standard` enum value. Any future option must be an enum or a strictly validated value. Free-form compiler flags are not accepted.
- **Least privilege in CI.** Workflows default to `contents: read`. Write scopes (for example `contents: write` for creating releases) are granted per job.
- **Supply chain.** Dependency updates arrive through Dependabot pull requests and pass dependency review. New dependencies need a justification in the pull request: why it is needed, its licence, its maintenance status, and alternatives considered.

## 5. Testing standards

[TESTING_STRATEGY.md](TESTING_STRATEGY.md) describes the test layers. The standards each test must meet:

- **One behaviour per test**, named for that behaviour (`BinarySearchWhileLoopIsUnknownNotGuessed`, not `Test3`).
- **Assert on semantics, not formatting**, where possible: compare the `bound_status`, the asymptotic class and the workload value. Compare a rendered expression string only when the rendering itself is under test.
- **Floating-point comparisons** use explicit tolerances (`EXPECT_NEAR`, or relative error) with a comment explaining the tolerance.
- **Hermetic.** No network, no access to the user's home directory, no dependence on the installed VS Code, and temporary files only in a test-owned temporary directory.
- **Fast by default.** Unit tests should run in milliseconds. Integration tests that invoke Clang parse small fixtures. Long-running tests get their own label and are kept out of the default run.
- **Failure messages must be diagnosable.** Use `<< "context"` on GoogleTest assertions and descriptive messages in `node:test`.

## 6. Change management

- Every non-trivial change starts from an issue with acceptance criteria. The backlog is in [IMPLEMENTATION_PLAN.md](../development/IMPLEMENTATION_PLAN.md).
- Architectural decisions (a new dependency, a protocol major version, a new process boundary, a platform policy) need an ADR.
- Specification documents are the source of truth for behaviour. If the code and the specification disagree, file an issue and fix one of them. Do not leave the drift in place.
