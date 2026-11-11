# ADR 0012: Re-architect the native engine in Rust

- **Status:** Accepted
- **Date:** 2026-11-11

## Context

BigO Lens performs semantic analysis of competitive programming source code (trip counts, asymptotic complexity, recurrence solving, workload bounds under linear/sum/product constraints, and memory feasibility). The original engine was built in C++20 with Clang LibTooling (see [ADR 0001](0001-clang-ast-instead-of-regex.md) and [ADR 0002](0002-separate-native-engine.md)).

While C++20 provided direct access to Clang's C++ AST classes, building the core symbolic algebra, recurrence solvers, and constraint evaluation in C++ presented several challenges:
- Complex memory management and object lifetime constraints for recursive symbolic expression trees (`std::shared_ptr<const Expr>`).
- Lack of first-class algebraic data types (discriminated unions) and exhaustive pattern matching, necessitating manual `switch` and type casts across AST expression transformations.
- Build system and dependency friction: CMake build times, cross-platform packaging hurdles, and compiler warning differences across GCC, Clang, and MSVC.
- Strict requirements for memory safety and zero undefined behavior in the symbolic evaluation and constraint solvers.

The project requires an engine that is memory-safe, maintainable, modular, fast, and easily testable, while maintaining exact compatibility with the versioned JSON protocol ([ADR 0004](0004-json-ipc.md)) and preserving semantic fidelity for C++ source analysis.

## Decision

Re-architect the standalone engine executable (`bigolens-engine`) in **Rust** within a modular Cargo workspace, executed via a phased migration:

1. **Workspace Architecture (`crates/`)**:
   - **`crates/bigolens-core`**: Pure Rust library. Contains:
     - Protocol domain models (strongly typed, serialised via `serde` with `#[serde(rename_all = "snake_case")]` to match `shared/schemas/`).
     - Symbolic expression trees (`enum Expr`) with algebraic simplifications and asymptotic ordering.
     - Recurrence solver (Master Theorem, linear recurrences, Akra-Bazzi).
     - Constraint and workload evaluation (linear programming, sum-constraint packing, product bounds).
     - Feasibility evaluation (time and memory models).
     - Abstract analyzer trait (`pub trait SourceAnalyzer`).
     - `#![forbid(unsafe_code)]`.
   - **`crates/bigolens-clang`**: Optional C++ AST frontend crate. Uses Clang LibTooling via FFI (`cxx` or `clang-sys`) to traverse C++ ASTs with full semantic resolution (types, sizes, template arguments, constant evaluation) and map them to `bigolens-core` models.
   - **`crates/bigolens-engine`**: Executable binary crate.
     - CLI argument parsing via `clap`.
     - Request dispatching and protocol JSON handling.
     - Communicates over stdin/stdout or `--request <file>` JSON IPC.

2. **Integration with VS Code Client**:
   - The VS Code extension remains a lightweight TypeScript client ([ADR 0003](0003-typescript-extension.md)).
   - It invokes the compiled `bigolens-engine` binary through the existing versioned JSON protocol (`v1.0.0`), with zero breaking changes to editor UI, commands, or settings.

3. **Phased Migration Strategy**:
   - Maintain the existing C++ engine and tests as the ground truth reference during migration.
   - Implement `bigolens-core` in Rust, validating unit tests and property tests against the reference implementation.
   - Integrate the Clang frontend in `crates/bigolens-clang` to achieve test parity against `tests/fixtures/cpp/`.
   - Switch default binary packaging in release pipelines once feature parity and verification pass.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Complete rewrite in Rust without Clang (Tree-sitter only) | Tree-sitter provides syntax only: no semantic types, no constant evaluation, no sizeof resolution, and no template instantiation. This would violate the prime directive ([RULES.md](../../../RULES.md) §0) of result honesty. |
| Engine in TypeScript / WebAssembly | Native compilation is necessary for large C++ AST parsing and CPU-pinned benchmarking. WASM memory and system header access remain cumbersome. |
| Continue purely in C++20 | Slower development velocity for symbolic algebra, lack of native pattern matching, and increased risk of memory safety bugs in complex expression manipulation. |

## Consequences

**Positive**
- **Safety**: Memory safety guaranteed by the Rust compiler. `#![forbid(unsafe_code)]` in `bigolens-core`.
- **Expressiveness**: Rust `enum` and exhaustive pattern matching simplify symbolic expression trees and asymptotic simplifiers significantly.
- **Modern Tooling**: Cargo handles dependencies, formatting (`cargo fmt`), linting (`cargo clippy`), and documentation generation seamlessly.
- **Protocol Parity**: Serde ensures bidirectional compatibility with the existing JSON schema and TypeScript extension.

**Negative**
- Requires Rust toolchain installation in development and CI environments.
- FFI boundary between Rust and Clang C++ LibTooling in `bigolens-clang` requires careful interface definition (`cxx`).

## Follow-up considerations

- Configure GitHub Actions CI to include `cargo test` and `cargo clippy`.
- Explore compiling `bigolens-core` to WebAssembly (`wasm32-unknown-unknown`) as an optional browser-based evaluator in the future.
