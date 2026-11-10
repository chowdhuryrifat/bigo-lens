# Coding Standards

This document gives concrete style rules and examples for BigO Lens. Naming and structural rules are defined in [RULES.md](../../RULES.md). This document shows how to apply them and documents the tool configuration that enforces them.

Formatting is automated. Run `scripts/development/format.sh` (clang-format and Prettier) before you commit, and let the tools settle style questions they cover.

## 1. Tooling overview

| Tool | Config file | Applies to | Enforced in CI |
| --- | --- | --- | --- |
| clang-format | [`.clang-format`](../../.clang-format) | `*.cpp`, `*.hpp` | `code-quality.yml` (format check) |
| clang-tidy | [`.clang-tidy`](../../.clang-tidy) | engine, benchmark, tests (Linux, via `compile_commands.json`) | `code-quality.yml` |
| Compiler warnings | [`cmake/CompilerWarnings.cmake`](../../cmake/CompilerWarnings.cmake) | all C++ targets | `code-quality.yml` configures with `-Werror` |
| ESLint (flat config, typescript-eslint `strictTypeChecked`) | [`extension/eslint.config.mjs`](../../extension/eslint.config.mjs) | `extension/**/*.ts`, `*.mjs` | `ci.yml`, `code-quality.yml` |
| TypeScript compiler | [`extension/tsconfig.json`](../../extension/tsconfig.json) | extension sources and tests | `ci.yml` (`npm run compile`) |
| Prettier | [`.prettierrc`](../../.prettierrc), [`.prettierignore`](../../.prettierignore) | TS, JS, JSON, YAML (Markdown is ignored) | `code-quality.yml` (`npm run format:check`) |
| EditorConfig | [`.editorconfig`](../../.editorconfig) | all files | editor-level |

## 2. C++ style

### 2.1 Formatting (`.clang-format`)

The configuration is Google style with these deviations:

```yaml
BasedOnStyle: Google
Standard: c++20
ColumnLimit: 120
IndentWidth: 2
AccessModifierOffset: -1        # "public:" indented by one space
DerivePointerAlignment: false
PointerAlignment: Left          # const Expr* expr, not const Expr *expr
IncludeBlocks: Preserve         # sorts within blocks; you create the blocks
SortIncludes: CaseSensitive
AllowShortFunctionsOnASingleLine: Inline
AllowShortIfStatementsOnASingleLine: Never
AllowShortLoopsOnASingleLine: false
```

In practice this means:

- Lines are at most 120 columns, indented with 2 spaces, never tabs.
- `if`, `for` and `while` bodies always go on their own line, and always in braces (`readability-braces-around-statements` is enabled in clang-tidy).
- Short member functions defined in the class body may stay on one line.

### 2.2 Include order

```cpp
#include "bigolens/api/request_handler.hpp"      // 1. related header

#include <exception>                              // 2. C++ standard library
#include <optional>

#include <nlohmann/json.hpp>                      // 3. third-party

#include "bigolens/api/protocol.hpp"              // 4. project headers
#include "bigolens/constraints/workload.hpp"
```

Because `IncludeBlocks: Preserve` is set, clang-format sorts each block but never moves a header between blocks. Keep the blank lines.

### 2.3 Header template

```cpp
#pragma once

#include <optional>
#include <string>

#include "bigolens/complexity/expression.hpp"
#include "bigolens/constraints/constraint_set.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::constraints {

/// Worst-case structural workload of an expression under constraints.
struct WorkloadResult {
  models::BoundStatus status = models::BoundStatus::Unknown;
  std::optional<double> upperBound;  ///< Present when status is Proven or Estimated.
  std::string method;                ///< e.g. "pointwise-maxima", "sum-constraint-packing".
  std::vector<std::string> assumptions;
  std::vector<std::string> missing;  ///< Variables lacking an upper bound.
};

/// Computes an upper bound of `expr` under `constraints`.
/// Every free variable needs an upper bound, otherwise the status is Unknown.
[[nodiscard]] WorkloadResult evaluateWorkload(const complexity::Expr& expr, const ConstraintSet& constraints);

}  // namespace bigolens::constraints
```

This is abridged from `engine/include/bigolens/constraints/workload.hpp`, which is the authoritative version. The `<vector>` include is omitted here for brevity.

Points to note:

- `#pragma once` first, then includes, then a single nested namespace declaration (`namespace bigolens::constraints {`).
- Close the namespace with a `// namespace ...` comment.
- Use `///` doc comments for contracts and `///<` for trailing member documentation.
- Use default member initializers instead of constructor boilerplate for plain data structs.

### 2.4 Source file template

```cpp
#include "bigolens/feasibility/feasibility.hpp"

#include <string>

namespace bigolens::feasibility {

namespace {

/// Fraction of the time limit below which a pessimistic estimate is "likely safe".
/// Defined in docs/specifications/FEASIBILITY_MODEL.md.
constexpr double kLikelySafeFraction = 0.5;

std::string describe(double value) { /* ... */ }

}  // namespace

TimeAssessment assessTime(/* ... */) {
  // ...
}

}  // namespace bigolens::feasibility
```

### 2.5 Classes

```cpp
class RequestHandler {
 public:
  /// `analyzer` may be null (engine built without a front end); not owned.
  explicit RequestHandler(analyzers::SourceAnalyzer* analyzer) noexcept : analyzer_(analyzer) {}

  /// Never throws. Every failure is returned as a structured error response.
  [[nodiscard]] nlohmann::json handleText(std::string_view requestText) const;

 private:
  analyzers::SourceAnalyzer* analyzer_;  // non-owning
};
```

- Order members as: `public`, then `protected`, then `private`. Within each section put types first, then constructors and destructor, then methods, then data.
- Make single-argument constructors `explicit`.
- Mark overrides with `override` (enforced by `modernize-use-override`) and never repeat `virtual` on them.
- Data members are `private` with a trailing underscore. Plain aggregates (`struct`) have public fields without underscores.

### 2.6 Casting, nulls and initialization

- Use `static_cast` and friends. C-style casts fail the build (`-Wold-style-cast`).
- Use `nullptr`, never `NULL` or `0` (`modernize-use-nullptr`).
- Initialize every variable at its declaration (`cppcoreguidelines-init-variables`).
- In Clang code, use `llvm::dyn_cast<clang::ForStmt>(stmt)` and `llvm::dyn_cast_or_null<...>`. Never use `dynamic_cast`, because the translation unit is built with `-fno-rtti`.
- Prefer `const auto*` when the pointee type is obvious from the right-hand side.

### 2.7 Errors

```cpp
// Expected failure: return a status, do not throw.
if (!constraints.variables.contains(name)) {
  result.status = models::BoundStatus::Unknown;
  result.missing.push_back(name);
  return result;
}

// Invariant violation in a factory: throw. The caller guarantees validity.
if (!std::isfinite(base) || base <= 1.0) {
  throw std::invalid_argument("logarithm base must be a finite constant greater than 1");
}
```

At the request boundary, `RequestHandler` catches `std::exception`, writes a one-line message to stderr, and returns an `internal_error` response. Do not add further catch-all handlers deeper in the code.

### 2.8 clang-tidy (`.clang-tidy`)

Checks are enabled incrementally, and **every enabled check is an error** (`WarningsAsErrors: "*"`). The enabled set is:

- `bugprone-*`, except `easily-swappable-parameters` and `exception-escape`
- `cert-err33-c`, `cert-err58-cpp`
- `clang-analyzer-*`
- `cppcoreguidelines-init-variables`, `cppcoreguidelines-slicing`
- `misc-definitions-in-headers`, `misc-redundant-expression`, `misc-unused-using-decls`
- `modernize-use-nullptr`, `modernize-use-override`
- `performance-*`, except `performance-enum-size`
- `readability-braces-around-statements`, `readability-container-size-empty`, `readability-redundant-string-init`

Diagnostics are reported for headers that match `(engine|benchmark|tests)/.*\.(hpp|h)$`. To enable a new check, open a pull request that turns it on **and** fixes every finding it reports. Use `// NOLINT(check-name): reason` only with a reason, and never for a whole file.

To run clang-tidy locally (Linux, after a `dev` configure that produced `build/dev/compile_commands.json`):

```bash
clang-tidy -p build/dev engine/src/constraints/workload.cpp
```

### 2.9 Tests (GoogleTest)

```cpp
// From engine/tests/workload_test.cpp; multiTestCase() sets t <= 10, n <= 2e5, sum of n <= 2e5.
TEST(Workload, SumConstraintPreventsTTimesNSquared) {
  // t * n^2 with t <= 10, n <= 2e5, sum n <= 2e5: worst case is one case of size 2e5.
  const auto e = multiply({variable("t"), power(variable("n"), 2)});
  const auto r = evaluateWorkload(*e, multiTestCase());
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 4e10);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}
```

- The suite name is the unit (`Workload`, `Protocol`). The test name is the behaviour in `PascalCase`.
- Use `ASSERT_*` before dereferencing optionals, and `EXPECT_*` otherwise.
- Test files may use `using namespace bigolens::...;` at file scope. Production code may not.

## 3. TypeScript style

### 3.1 Compiler settings (`extension/tsconfig.json`)

`strict`, `noUnusedLocals`, `noUnusedParameters`, `noImplicitReturns`, `noFallthroughCasesInSwitch`, `exactOptionalPropertyTypes` and `noUncheckedIndexedAccess`. The target is ES2022 with CommonJS modules, and the output goes to `out/`.

Two of these settings affect everyday code:

- With `noUncheckedIndexedAccess`, `record[key]` has type `T | undefined`, so you must handle the undefined case.
- With `exactOptionalPropertyTypes`, `{ path?: string }` does not accept `path: undefined`. Omit the property instead.

### 3.2 Lint (`extension/eslint.config.mjs`)

The config uses `@eslint/js` recommended and `typescript-eslint` `strictTypeChecked`, with these additions:

- `no-console: error`. Use the output channel instead. Build scripts (`*.mjs`) are exempt.
- `eqeqeq: always`
- `restrict-template-expressions` allows numbers.

### 3.3 Formatting (`.prettierrc`)

```json
{ "printWidth": 110, "singleQuote": true, "trailingComma": "all", "semi": true, "endOfLine": "lf" }
```

Prettier ignores Markdown, generated output (`extension/out/`, `extension/schemas/`), `shared/examples/*.response.json` (engine-generated) and the lock file.

### 3.4 Patterns

**Validate at the boundary and type the result:**

```ts
const validate = ajv.compile<AnalyzeResponse>(responseSchema);

export function parseEngineResponse(stdout: string): EngineResponse {
  let value: unknown;
  try {
    value = JSON.parse(stdout);
  } catch (err) {
    throw new EngineProtocolError('engine wrote invalid JSON', { cause: err });
  }
  if (!validate(value)) {
    throw new EngineProtocolError(`response failed schema validation: ${ajv.errorsText(validate.errors)}`);
  }
  return value;
}
```

**Spawn without a shell, with a timeout and an output cap:**

```ts
const child = spawn(enginePath, [], { shell: false, stdio: ['pipe', 'pipe', 'pipe'] });
child.stdin.end(JSON.stringify(request));
// Collect stdout up to MAX_STDOUT_BYTES (16 MiB); kill on overflow or on timeout.
```

**Exhaustive switches over protocol enums:**

```ts
function label(status: BoundStatus): string {
  switch (status) {
    case 'proven':
      return 'Proven';
    case 'estimated':
      return 'Estimated';
    case 'unknown':
      return 'Unknown';
    case 'unsupported':
      return 'Unsupported';
  }
}
```

These snippets illustrate the patterns. The function and class names are examples, not a reference to specific source files.

### 3.5 Tests (`node:test`)

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';

test('engine path setting takes precedence over BIGOLENS_ENGINE_PATH', () => {
  // arrange a fake filesystem / environment, act, assert
});
```

- Unit tests live in `extension/test/unit/` and must not import `vscode`.
- Integration tests (`extension/test/integration/`) run against the real engine and **skip with an explicit message** when no engine is found.
- VS Code smoke tests (`extension/test/e2e/`) run in the extension host through `@vscode/test-electron`.

## 4. CMake style

- Use lowercase commands and two-space indentation, with one argument per line when the call wraps, matching the existing files.
- Name targets `bigolens_<name>`, with an `ALIAS bigolens::<name>`, and link to the alias from other directories.
- Use `target_*` commands only. No global `include_directories`, `add_definitions` or `link_libraries`.
- Every target calls `bigolens_set_warnings(<target>)`.
- Options are named `BIGOLENS_<FEATURE>` and declared in the root `CMakeLists.txt`.
- Nothing machine-specific. Paths come from `find_package`, cache variables or presets.

## 5. JSON, YAML and shell

- JSON uses 2-space indentation and is formatted by Prettier. Example response files are written by the engine and excluded from Prettier.
- JSON Schemas declare `"$schema": "https://json-schema.org/draft/2020-12/schema"` and `additionalProperties: false` on every object.
- GitHub workflow YAML is formatted by Prettier. Each job declares `permissions`.
- Shell scripts start with `#!/usr/bin/env bash` and `set -euo pipefail`, quote every expansion, resolve the repository root from the script location (not the caller's working directory), and never require root.
- Line endings are LF everywhere except `*.cmd`, `*.bat` and `*.ps1` (CRLF). This is set in both `.editorconfig` and `.gitattributes`.
