# ADR 0013: Multi-Language Complexity Analysis and Pluggable Frontends

- **Status:** Accepted (Supersedes [ADR 0010](0010-cpp-only-mvp.md))
- **Date:** 2026-11-12

## Context

BigO Lens was originally conceived to provide static complexity analysis for competitive programming. In [ADR 0010](0010-cpp-only-mvp.md), the scope was restricted exclusively to C++ because the prototype was built in C++20 directly linked to Clang LibTooling.

However, the primary mission of BigO Lens is to **compute the time and space complexity of code regardless of programming language**. Algorithms and algorithmic patterns—such as binary search, two-pointer sweeps, nested loops, divide-and-conquer recurrences, sorting, and dynamic programming grids—are universal mathematical concepts that share identical asymptotic behavior across C++, Python, Java, Rust, Go, and JavaScript.

With the engine re-architected in Rust ([ADR 0012](0012-rust-engine-architecture.md)), the core mathematical engine (`crates/bigolens-core`: symbolic expression trees, asymptotic simplifier, Master Theorem solver, constraint optimizer, and feasibility models) is 100% decoupled from any specific source language or compiler runtime.

## Decision

1. **Supersede ADR 0010**:
   - BigO Lens expands from C++-only to **multi-language complexity analysis**.
   - Target languages in the initial rollout: **C++**, **Python**, **Java**, and **Rust**.

2. **Language-Agnostic Semantic IR (`bigolens-core::ir`)**:
   - `bigolens-core` defines a language-neutral intermediate representation for control flow and storage:
     - `LoopNest`: Loop induction variable, initial value, terminal bound expression, step expression, direction, condition, early exits, and child statements/nests.
     - `RecursiveFunction`: Function name, parameter list, base cases, recursive call sites, parameter reductions (e.g. $n/b$, $n - c$), and straight-line work per activation.
     - `StorageAllocation`: Element counts, buffer kinds (arrays, vectors, lists, dynamic tables), and element size estimates.
     - `LibraryCall`: Standard algorithmic operations (sorting, binary search, map/set lookups, heap operations) with language-specific cost tables.

3. **Pluggable Frontend Architecture**:
   - Frontends implement the `SourceAnalyzer` trait:
     ```rust
     pub trait SourceAnalyzer: Send + Sync {
         fn can_analyze(&self, language: &str) -> bool;
         fn analyze(&self, source: &SourcePayload, constraints: &ConstraintSet) -> Result<SourceAnalysis, AnalysisError>;
     }
     ```
   - **Tree-sitter Unified Parser (`crates/bigolens-analyzer`)**:
     - Uses pure Rust Tree-sitter parsers (`tree-sitter-cpp`, `tree-sitter-python`, `tree-sitter-java`, `tree-sitter-rust`).
     - Fast, memory-safe, syntax-error tolerant, and operates with zero external compiler dependencies.
     - Maps syntactic loop constructs (`for`, `while`, range-for), function definitions, recursive calls, and collection instantiations to the Language-Agnostic IR.
   - **Clang Semantic Frontend (`crates/bigolens-clang`)**:
     - Preserved as an optional high-precision C++ analyzer for complex template metaprogramming and exact platform type layouts.

4. **Protocol & Extension Multi-Language Support**:
   - Protocol request `source.language` enum expands: `"cpp" | "python" | "java" | "rust"`.
   - VS Code extension registers language selectors for C++ (`cpp`, `c`), Python (`python`), Java (`java`), and Rust (`rust`).

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Maintain C++ only | Fails to serve the millions of developers and students solving algorithms in Python, Java, and Rust. |
| Separate engine per language | Massive code duplication. The complexity algebra ($O(n \log n)$, recurrences, sum-constraint packing) is identical regardless of source language. |
| Language Server Protocol (LSP) queries only | LSP does not provide structural AST loop bounds, induction variable steps, or recurrence graphs. |

## Consequences

**Positive**
- **Universal Utility**: One analyzer calculates Big-O complexity for Python, Java, Rust, and C++ code.
- **Portability**: Tree-sitter runs on all platforms (Linux, macOS, Windows, WebAssembly) with zero external toolchain requirements.
- **Unified Engine**: `bigolens-core` solves all complexity, constraint, and feasibility math identically.

**Negative**
- Requires writing Tree-sitter AST visitor extractors for each supported language.
