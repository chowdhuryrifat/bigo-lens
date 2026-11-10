# ADR 0001: Use the Clang AST instead of regular expressions or a custom parser

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

BigO Lens derives loop trip counts, recursion and storage sizes from C++ source. To do this correctly it needs several kinds of information:

- Scoping and name resolution, for example to tell whether the `n` in `i < n` is the same variable that is written in the body.
- Types and their sizes, for exact `sizeof` of globals and arrays.
- Constant evaluation, for `const` and `constexpr` bounds.
- Templates, macros, overloaded operators, range-for desugaring, and the distinction between user-defined and library calls.

Text-based approaches handle none of these reliably. Their failures are silent: a regex that misreads a loop produces a confident but wrong O() bound. That directly violates the project's honesty rules in [RULES.md](../../../RULES.md).

## Decision

Analyze C++ with **Clang LibTooling** on a real AST:

- Parse with `-fsyntax-only` on an in-memory file.
- Use a fixed argument list. Only the validated `cpp_standard` comes from the request.
- Suppress warnings and collect at most 50 error diagnostics.
- **Never analyze an AST that has errors.** Such a request returns `source_parse_error` with the diagnostics.

All Clang use is confined to `engine/src/parser/clang_frontend.cpp`, behind the `analyzers::SourceAnalyzer` interface and the `parser::makeClangSourceAnalyzer()` factory.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Regular expressions or token heuristics | No scoping, types or macro expansion. They fail silently on real code. |
| tree-sitter grammar | Fast and robust to errors, but it gives syntax only: no semantic types, no constant evaluation, no resolution of `sizeof` or overloads. |
| Custom C++ parser | The cost is unbounded and it would never match the language. |
| GCC plugin | Less stable plugin API, and harder to embed as a library. |
| clangd or LSP queries | Too coarse for loop-structure analysis, and it adds a dependency on a server. |

## Consequences

**Positive**

- Semantic precision: `mayModify` checks on real `VarDecl`s, exact type layout for storage, and the ability to tell library callees from user-defined ones.
- Compiler-grade diagnostics for invalid code.
- The core stays testable without Clang (see [ADR 0002](0002-separate-native-engine.md)).

**Negative**

- A large build-time dependency. LLVM and Clang must be installed and found with `find_package(Clang CONFIG)`, and `Clang_DIR` may need to be set.
- Platform friction. The frontend is primary on Linux, experimental on macOS (Homebrew LLVM) and experimental or untested on Windows. CI builds the core without Clang on macOS and Windows.
- Headers named in `#include` are read from the user's filesystem, so analysis depends on the local toolchain's headers.
- Clang API changes between LLVM major versions can require code updates. CI uses LLVM 18, and development has used Clang 20.

## Follow-up considerations

- Decide on a supported LLVM version range and test the oldest and newest in CI.
- Consider shipping or detecting a matching Clang resource directory (`ClangFrontendOptions::resourceDir`).
- Evaluate error-tolerant analysis for partially invalid files only if it can be done without guessing.
