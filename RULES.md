# BigO Lens Engineering Rules

This is the rulebook for everyone who changes BigO Lens, whether human or AI agent. The rules are mandatory unless a rule says otherwise. If a rule blocks a good change, open an issue or an ADR in [`docs/architecture/DECISIONS/`](docs/architecture/DECISIONS/) to change it. Do not quietly ignore it.

The rules here are short. These documents give the details:

- [docs/engineering/ENGINEERING_RULES.md](docs/engineering/ENGINEERING_RULES.md): maintainability, quality, performance, security and testing standards
- [docs/engineering/CODING_STANDARDS.md](docs/engineering/CODING_STANDARDS.md): concrete style, examples and tool configuration
- [docs/engineering/GIT_WORKFLOW.md](docs/engineering/GIT_WORKFLOW.md): branches, commits, pull requests and releases
- [docs/engineering/TESTING_STRATEGY.md](docs/engineering/TESTING_STRATEGY.md): test layers, fixtures and negative tests
- [docs/engineering/THREAT_MODEL.md](docs/engineering/THREAT_MODEL.md): trust boundaries and security requirements
- [docs/development/DEFINITION_OF_DONE.md](docs/development/DEFINITION_OF_DONE.md): the checklist every change must meet

## 0. The prime directive: honesty of results

BigO Lens is only useful if users can trust what it reports. That puts one rule above every other:

> **The engine never reports a complexity, workload, memory figure or feasibility verdict that it cannot justify.** When it cannot analyse something, it says `unknown` or `unsupported` and explains why. It never guesses.

Every result carries `bound_status` (`proven | estimated | unknown | unsupported`), `confidence`, `assumptions[]` and `limitations[]`. Fill these in truthfully. A change that makes a result look more certain than the analysis supports is a bug, even if it makes a demo look better.

The same rule applies to documentation, commit messages and pull request descriptions. Describe what the code does today. Mark planned work as planned.

## 1. C++ rules (engine and benchmark)

### 1.1 Language and build

- Use C++20 (`CMAKE_CXX_STANDARD 20`, extensions off). CMake 3.24 or newer.
- All code must compile without warnings under the project warning policy in [`cmake/CompilerWarnings.cmake`](cmake/CompilerWarnings.cmake): `-Wall -Wextra -Wpedantic -Wshadow -Wconversion -Wsign-conversion -Wnon-virtual-dtor -Wold-style-cast -Woverloaded-virtual` on GCC/Clang (plus `-Wnull-dereference` on Clang; GCC reports false positives in libstdc++ and LLVM headers at `-O2`), and `/W4 /permissive-` on MSVC. CI configures with `BIGOLENS_WARNINGS_AS_ERRORS=ON`.
- Apply `bigolens_set_warnings(<target>)` to every new target.
- `bigolens_core` must **not** depend on Clang or LLVM. Only `engine/src/parser/clang_frontend.cpp` may include Clang headers. Front-end types must never appear in a public header. The boundary is the abstract interface `analyzers::SourceAnalyzer`.
- The engine must build and pass its tests with `BIGOLENS_ENABLE_CLANG=OFF` (the `no-clang` preset). This is how Windows and macOS CI run.

### 1.2 Types and interfaces

- Use strong enums (`enum class`) for every status or category that crosses a module boundary (`BoundStatus`, `Confidence`, `FeasibilityStatus`, `Severity`, `FindingCategory`). Convert them to protocol strings in exactly one place (`models::toString`).
- Mark functions `[[nodiscard]]` when they return results or status that a caller could ignore by mistake.
- Use `std::optional` for "no value" and `std::string_view` for read-only string parameters that are not stored.
- Use fixed-width integers (`std::int64_t`, `std::uint64_t`) for quantities that come from or go to the protocol.
- Arithmetic on sizes or byte counts that come from user input must be checked or saturating. Memory totals already saturate; keep that behaviour.
- Use floating point (IEEE double) only where the specifications allow it, for example the symbolic evaluator. Report non-finite results as `Overflow`. Never return them as numbers.

### 1.3 Clang front end

- The Clang translation unit is compiled with `-fno-rtti` whenever LLVM was built without RTTI, which is the default for distribution packages. **Do not use `dynamic_cast` or `typeid` on Clang types.** Use `llvm::isa`, `llvm::dyn_cast`, `llvm::dyn_cast_or_null` and `llvm::cast`.
- Parse with `-fsyntax-only`, an in-memory file and a fixed argument list. The validated `cpp_standard` is the only part of the request that may reach the compiler arguments.
- Never run analysis on an AST that has errors. Report `source_parse_error` with the collected diagnostics (at most 50) instead.
- Status rules for loop analysis are specified (see [docs/specifications/COMPLEXITY_MODEL.md](docs/specifications/COMPLEXITY_MODEL.md)). A new pattern must state its `bound_status` and `confidence` rule, and the pattern needs a fixture.

## 2. TypeScript rules (VS Code extension)

- Use TypeScript in `strict` mode, targeting Node 20 and VS Code `^1.85`. Do not use `any`. Use `unknown` and narrow it.
- Validate every value that crosses a process or file boundary (engine responses and sidecar constraint files) with ajv against the shared JSON Schemas before use. Never cast an unvalidated response to a typed interface.
- Spawn processes only with `child_process.spawn(executable, args, { shell: false })`. Do not use `exec`, `execSync` or `shell: true`, and do not build command strings.
- Give every engine invocation a timeout (`bigoLens.analysisTimeoutMs`) and an output cap (16 MiB). Kill the process when either is exceeded.
- Register disposables with `context.subscriptions`. No global mutable singletons outside the activation context.
- Keep VS Code API usage in `commands/` and `providers/`. Keep `services/` and `models/` testable without the VS Code runtime wherever practical.
- Use `async`/`await`. Never leave a promise floating. ESLint's `no-floating-promises` rule enforces this.

## 3. Naming conventions

### C++

| Element | Convention | Example |
| --- | --- | --- |
| Namespaces | `lower_case`, rooted at `bigolens::<module>` | `bigolens::constraints`, `bigolens::api` |
| Types (classes, structs, enums, aliases) | `PascalCase` | `ConstraintSet`, `WorkloadResult`, `ExprPtr` |
| Enumerators | `PascalCase` | `BoundStatus::Proven` |
| Functions and methods | `camelCase` | `evaluateWorkload`, `frontendVersion` |
| Variables and parameters | `camelCase` | `requestId`, `maxLength` |
| Private data members | `camelCase` with a trailing underscore | `analyzer_`, `options_` |
| Public struct fields | `camelCase`, no underscore | `upperBound`, `displayPath` |
| Constants (`constexpr`, `const` at namespace scope) | `k` + `PascalCase` | `kMaxJsonDepth`, `kMaxRequestBytes` |
| Macros (avoid them) | `BIGOLENS_UPPER_CASE` | `BIGOLENS_HAS_CLANG` |
| Files | `snake_case.hpp` / `snake_case.cpp` | `request_handler.cpp`, `growth_fit.hpp` |
| Tests | `<unit>_test.cpp` | `workload_test.cpp` |
| CMake targets | `bigolens_<name>` with alias `bigolens::<name>` | `bigolens_core` / `bigolens::core` |

### TypeScript

| Element | Convention | Example |
| --- | --- | --- |
| Variables, functions, methods | `camelCase` | `locateEngine`, `buildRequest` |
| Classes, interfaces, type aliases, enums | `PascalCase` | `EngineClient`, `AnalyzeResponse` |
| Module-level constants | `camelCase`, or `UPPER_SNAKE_CASE` for true compile-time constants | `MAX_STDOUT_BYTES` |
| Files | `camelCase.ts` | `engineLocator.ts`, `constraintsFile.ts` |
| Test files | `<module>.test.ts` | `engineLocator.test.ts` |
| Command IDs | `bigoLens.<verbPhrase>` | `bigoLens.analyzeActiveFile` |
| Settings | `bigoLens.<camelCase>` | `bigoLens.analysisTimeoutMs` |

### Protocol and data files

- JSON protocol keys: `snake_case` (`request_id`, `bound_status`).
- Enum string values: `snake_case` or `kebab-case` exactly as the schema defines them (`not_implemented`, `sum-constraint-packing`).
- Fixtures: `NN_descriptive_name.cpp` for positive cases and `neg_descriptive_name.cpp` for negative cases.

## 4. Header organization

- Every header begins with `#pragma once`. Do not use include guards.
- Public headers live in `engine/include/bigolens/<module>/<name>.hpp` (and `benchmark/include/bigolens/benchmark/`). The module directory matches the namespace: `bigolens/constraints/workload.hpp` declares into `bigolens::constraints`.
- Implementations live in `engine/src/<module>/<name>.cpp`.
- Headers include what they use and nothing more. Forward-declare where that avoids a heavy include.
- File-local helpers go in an anonymous namespace inside the module namespace. Never use `static` free functions in headers.
- Public headers contain no `using namespace` directives. Function-local or file-local `using` declarations in `.cpp` files are fine (`using nlohmann::json;`).
- Generated headers (`version.hpp`) are produced by CMake into the build tree. Never commit generated files.

## 5. Include ordering

Order includes in groups separated by a blank line, sorted alphabetically within each group:

1. The related header, for example `"bigolens/api/request_handler.hpp"` in `request_handler.cpp`
2. C++ standard library: `<optional>`, `<string>`
3. Third-party: `<nlohmann/json.hpp>`, `<gtest/gtest.h>`, `<clang/AST/...>`, `<llvm/...>`
4. Project headers: `"bigolens/..."`

Use angle brackets for the standard library and third-party headers, and quotes with the full `bigolens/` path for project headers. Never use relative includes (`"../foo.hpp"`). clang-format (`IncludeBlocks: Preserve`) sorts includes within each block but does not create the blocks, so you must separate the groups yourself (see [CODING_STANDARDS.md](docs/engineering/CODING_STANDARDS.md#22-include-order)).

## 6. Memory ownership and smart pointers

- Use RAII for everything: memory, file handles, child processes and temporary directories.
- No owning raw pointers and no naked `new` or `delete`. Use `std::make_unique` and `std::make_shared`.
- `std::unique_ptr` is the default for exclusive ownership. Example: `parser::makeClangSourceAnalyzer()` returns `std::unique_ptr<analyzers::SourceAnalyzer>`.
- Use `std::shared_ptr<const T>` only for immutable shared structure. The symbolic expression tree (`complexity::ExprPtr`) is the model: nodes are immutable and shared, so sharing is safe and cycles cannot form.
- Raw pointers and references are **non-owning** observers. A raw pointer means "may be null" (for example `RequestHandler(analyzers::SourceAnalyzer* analyzer)`, where null means no front end). A reference means "never null".
- Types that own resources or represent polymorphic interfaces delete their copy and move operations explicitly unless they need them (see `SourceAnalyzer`).
- Polymorphic base classes have a public virtual destructor or a protected non-virtual destructor.
- Clang AST nodes are owned by the `ASTContext`. Never store pointers to them beyond the lifetime of the analysis of one translation unit.

## 7. Exception safety and error handling

- **`RequestHandler` never throws.** Every failure inside request handling becomes a structured error response (`internal_error` as a last resort). `main.cpp` has a final `catch (const std::exception&)` that logs to stderr and exits 1. That path is a bug indicator, not normal control flow.
- **Exceptions never cross the protocol boundary.** The extension sees JSON only.
- Inside the engine, throw exceptions for programming errors and invariant violations. For example, expression factories throw on a negative power exponent or a logarithm base of 1 or less. Use result types (`std::optional`, status structs such as `WorkloadResult`, `ProtocolError` lists) for expected failures such as invalid input, unbound variables or overflow.
- Validation collects **all** errors with JSON-pointer paths instead of stopping at the first one, so users can fix a request in one round trip.
- Catch exceptions by `const&`. Never use `catch (...)` without rethrowing or converting to a reported error.
- Destructors, move operations and `swap` are `noexcept`.
- Do not throw through Clang callbacks (AST visitors, diagnostic consumers). Convert errors to findings or diagnostics at the call site.
- In TypeScript, reject promises with `Error` subclasses carrying a stable `code`. Surface failures to the user through a notification or output channel message with an actionable hint.

## 8. Logging

- **Engine: stdout is reserved for exactly one JSON response line.** Nothing else may ever be written to stdout: no debug prints, no progress, no banners (`--version` and `--help` are the only exceptions, and they never send a request).
- Engine logging goes to **stderr** only, prefixed with `bigolens-engine:`. Log sparingly. The extension captures stderr for diagnostics.
- Never log full source content, constraint files or environment variables. Paths and error messages are acceptable.
- **Extension:** log to the `BigO Lens` output channel. Do not use `console.log` in shipped code. User-facing errors use `vscode.window.show*Message` and point to the output channel for details.

## 9. JSON protocol rules

The protocol is specified in [docs/specifications/JSON_PROTOCOL.md](docs/specifications/JSON_PROTOCOL.md). The schemas in [`shared/schemas/`](shared/schemas/) (JSON Schema draft 2020-12) are normative.

- One request in, one response out, as a single JSON document on one line.
- Requests carry `schema_version` as `"MAJOR.MINOR"`. The engine accepts the same major version with a minor version less than or equal to its own. Anything else returns `unsupported_schema_version`.
- Unknown fields are rejected (`additionalProperties: false`). This is deliberate: it keeps typos from being silently ignored.
- Every response contains `schema_version`, `request_id`, `status`, `engine`, `results`, `diagnostics` and `errors`, even on error.
- Error codes are a closed set (`malformed_json`, `request_too_large`, `invalid_request`, `unsupported_schema_version`, `not_implemented`, `frontend_unavailable`, `source_parse_error`, `internal_error`). Adding one is a minor protocol change. Changing the meaning of one is a breaking change.
- Input limits are enforced **before** expensive work: 4 MiB request, 2 MiB source, nesting depth 64 (checked before parsing), integer magnitude 10^18.
- Every change to the protocol updates, in the same pull request: the schemas, the C++ serializer and validator, the TypeScript types, [JSON_PROTOCOL.md](docs/specifications/JSON_PROTOCOL.md), and the examples in [`shared/examples/`](shared/examples/). Regenerate the response examples with `scripts/development/regenerate-examples.sh`. Never edit them by hand.
- Response values must validate against `response.schema.json`. The extension enforces this at runtime, and CI validates the examples (`npm run validate:schemas`).

## 10. API compatibility

- The project is pre-1.0 (`0.x`). The C++ library APIs are internal and may change freely. The **JSON protocol is the public contract** between the engine and any client.
- Protocol versioning:
  - **Minor** (`1.0` to `1.1`): additive and backward compatible, such as new optional request fields, new response fields, new enum values clients must tolerate, or new error codes.
  - **Major** (`1.x` to `2.0`): removing or renaming fields, changing types or semantics, or making optional request fields required.
- Clients must ignore response fields they do not know. The engine rejects request fields it does not know.
- The extension and engine version and ship together. The extension must check `engine.protocol_version` and report an incompatible engine clearly. It must not misparse the response.
- Settings and command IDs in `extension/package.json` are user-facing API. Renaming one requires a migration note in the changelog.

## 11. Testing

The full strategy is in [TESTING_STRATEGY.md](docs/engineering/TESTING_STRATEGY.md).

- Every behaviour change comes with tests. Every bug fix comes with a regression test that fails before the fix.
- Every new loop pattern, finding kind or status rule gets a C++ fixture in `tests/fixtures/cpp/` and an integration test asserting its pattern, expression, `bound_status` and `confidence`.
- Every new validation rule or error code gets a negative test.
- Tests are deterministic: no wall-clock assertions, no network, no dependence on test order, and no machine-specific paths. Use the `BIGOLENS_FIXTURE_DIR` compile definition and the presets.
- Ordinary CI must not contain performance thresholds.
- CTest tests are labelled `unit` or `integration`.
- Run the relevant suites locally before you push, and report what you ran in the pull request. Do not claim a test passes unless you ran it.

## 12. Documentation

- Code, specifications and documentation change together in the same pull request.
- Public headers carry `///` Doxygen-style comments that describe the contract: preconditions, units, ownership, and what is returned on failure. Do not narrate the implementation.
- Comments explain *why*. Do not leave commented-out code.
- Documentation states implementation status precisely. Use **Implemented**, **Partial**, **Planned (Mx)** or **Not implemented**. Copying an aspirational sentence from a design document into a user guide as if it were fact is a documentation bug.
- Major design decisions are recorded as ADRs in [`docs/architecture/DECISIONS/`](docs/architecture/DECISIONS/).
- Use relative links between documents. Check that the links resolve.

## 13. Security

See [SECURITY.md](SECURITY.md) and [THREAT_MODEL.md](docs/engineering/THREAT_MODEL.md).

- Treat all user source code, constraint files, requests and benchmark inputs as **untrusted**.
- The engine never executes user code today. Any feature that compiles or runs user code (M4 benchmarking) must be user-initiated and must follow the sandboxing requirements in the threat model before it is merged. A timeout alone is not a sandbox. Until real isolation exists, the user interface and documentation must state plainly that benchmarking is not sandboxed.
- Compiler and process arguments are built as an argv array from validated values. No request field may inject a compiler flag.
- Temporary files go in per-run directories created with secure APIs and cleaned up on every exit path.
- Dependencies are pinned. CMake downloads use pinned URLs with SHA-256. npm uses `package-lock.json` and `npm ci`. GitHub Actions use official actions only.
- CI workflows run with least privilege (`permissions: contents: read` unless a job needs more).

## 14. Git conventions

The details are in [GIT_WORKFLOW.md](docs/engineering/GIT_WORKFLOW.md).

- We use trunk-based development on `main`. Keep branches short-lived.
- Branch names: `feat/<topic>`, `fix/<topic>`, `docs/<topic>`, `chore/<topic>`, plus `refactor/`, `test/`, `ci/`, `perf/` where they fit. Use lowercase kebab-case topics, optionally with the issue ID (`feat/bl-205-recurrence-interface`).
- Commits follow [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`, imperative mood, at most 72 characters, no trailing period. Scopes: `engine`, `frontend`, `protocol`, `constraints`, `benchmark`, `extension`, `ci`, `docs`, `build`.
- Breaking protocol changes use `!` and a `BREAKING CHANGE:` footer.
- Never force-push to `main`. Never commit directly to `main` except through a merged pull request.
- Release tags are `vX.Y.Z` and must match the versions in `CMakeLists.txt` and `extension/package.json`.

## 15. Pull request standards

A pull request:

- Does one thing. Split unrelated changes.
- References its issue (`Closes #123` or the `BL-xxx` ID).
- Describes **what** changed, **why**, and **how it was verified**, listing the exact commands run.
- Passes CI: build matrix, tests, clang-format, clang-tidy, ESLint, Prettier, schema validation and CodeQL.
- Updates docs, schemas, examples and the changelog where applicable.
- Meets the [Definition of Done](docs/development/DEFINITION_OF_DONE.md).
- Has at least one approving review before merge. Authors do not approve their own pull requests.
- Is squash-merged, with a squash commit message that follows Conventional Commits.

## 16. Prohibited practices

These are rejected in review regardless of context:

1. **Regex or ad-hoc text matching as the primary C++ parser.** C++ analysis runs on the Clang AST. Text heuristics may only pre-filter or post-process, and never decide a complexity result.
2. **Unsupported complexity guesses.** Never emit an `O(...)`, a workload or a feasibility verdict that the analysis did not derive. Unrecognized shapes become `unknown` or `unsupported`, with a limitation explaining why. This also forbids rounding `estimated` up to `proven` and inventing default constraint values.
3. **Silent exception swallowing.** No empty `catch` blocks and no `catch (...) {}`. No `.catch(() => {})` in TypeScript. Every caught error is converted into a reported error or diagnostic, or rethrown.
4. **Unnecessary global mutable state.** No mutable globals or singletons. Configuration flows through parameters and objects. Immutable `constexpr` tables are fine.
5. **Machine-specific hardcoded paths.** No `/home/...`, `C:\Users\...` or `/usr/lib/llvm-18` in code, CMake, tests or committed settings. Use CMake variables, presets, compile definitions, environment variables or VS Code settings.
6. **Unvalidated external input.** Every request, sidecar file, engine response, environment variable and (in future) benchmark input is validated against its schema or limits before use.
7. **Fake benchmark results.** Never fabricate, hand-edit, cherry-pick or "illustrate" timings, fits or feasibility results, in code, tests, documentation or marketing. Example responses in `shared/examples/` are generated by the real engine.
8. **Unverified test claims.** Never state in a pull request, commit, issue or document that tests pass, that a platform is supported, or that coverage exists, unless you actually ran it and observed the result.
9. **Shell-string process spawning.** No `system()`, `popen()`, `exec()`, `shell: true` or string-concatenated command lines. Use argv arrays.
10. **Committing secrets.** No tokens, keys, credentials, `.env` files or personal data in the repository, test fixtures or CI logs. Secrets live in GitHub encrypted secrets and are scoped to the jobs that need them.
11. **Including Clang headers outside `clang_frontend.cpp`, or using RTTI on Clang types.**
12. **Writing anything to engine stdout other than the single JSON response.**
13. **Disabling a failing test, linter rule or warning to make CI green** without a linked issue and a justification comment.
