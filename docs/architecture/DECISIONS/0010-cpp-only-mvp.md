# ADR 0010: Support only C++ in the MVP

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Each language needs its own frontend, loop and recursion semantics, standard-library cost model and memory model. Supporting several languages early would spread effort thinly and lower the quality of every result. The primary audience (competitive programming and performance-sensitive code) predominantly uses C++.

## Decision

- The MVP supports **C++ only**. The protocol requires `source.language` to be `"cpp"`; other values are rejected with `invalid_request` at `/source/language` (test `NonCppLanguageIsRejected`).
- The supported standards are `c++11`, `c++14`, `c++17`, `c++20` and `c++23`, with `c++20` as the default, through `options.cpp_standard` and the `bigoLens.cppStandard` setting.
- The architecture keeps a path open for other languages without promising it. The core (`complexity`, `constraints`, `estimation`, `feasibility`, `models`) is language-agnostic, and frontends plug in through `analyzers::SourceAnalyzer`.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| C and C++ together from the start | C mostly works through Clang, but it has not been tested or specified. It can be added later deliberately. |
| Python or Java first | Different semantics and a different audience. A dynamic language makes static bounds much weaker. |
| Language-agnostic pattern matching | Contradicts [ADR 0001](0001-clang-ast-instead-of-regex.md). |

## Consequences

**Positive**

- Focused effort on one high-quality frontend, STL cost table and memory model.
- A simple protocol validation rule.

**Negative**

- Users of other languages are not served.
- Some core concepts, such as `v.size()` variable naming and the STL table, are C++-flavoured.

## Follow-up considerations

- Adding a language requires a new `SourceAnalyzer`, a new allowed `language` value (a minor protocol version), fixtures and a cost model.
- See [REQUIREMENTS](../../specifications/REQUIREMENTS.md) and [ROADMAP](../../development/ROADMAP.md).
