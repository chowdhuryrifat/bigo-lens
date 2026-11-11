# Architecture Decision Records

This directory records the significant architectural decisions for BigO Lens. Each ADR has a title, status, date, context, decision, alternatives considered, consequences and follow-up considerations.

| ADR | Title | Status | Date |
|---|---|---|---|
| [0001](0001-clang-ast-instead-of-regex.md) | Use the Clang AST instead of regular expressions or a custom parser | Accepted | 2026-11-10 |
| [0002](0002-separate-native-engine.md) | Run the analysis in a separate native engine | Accepted | 2026-11-10 |
| [0003](0003-typescript-extension.md) | Build the editor integration as a TypeScript VS Code extension | Accepted | 2026-11-10 |
| [0004](0004-json-ipc.md) | Use versioned JSON over stdin and stdout, one process per request | Accepted | 2026-11-10 |
| [0005](0005-symbolic-expression-trees.md) | Represent costs as immutable symbolic expression trees | Accepted | 2026-11-10 |
| [0006](0006-hybrid-analysis.md) | Combine static analysis with dynamic measurement (hybrid analysis) | Accepted | 2026-11-10 |
| [0007](0007-uncertainty-aware-runtime-estimates.md) | Express runtime estimates as uncertainty ranges, never as single predictions | Accepted | 2026-11-10 |
| [0008](0008-conservative-unknown-unsupported-results.md) | Report Unknown or Unsupported rather than guess | Accepted | 2026-11-10 |
| [0009](0009-independent-benchmark-subsystem.md) | Keep benchmarking as an independent subsystem | Accepted | 2026-11-10 |
| [0010](0010-cpp-only-mvp.md) | Support only C++ in the MVP | Superseded by 0013 | 2026-11-10 |
| [0011](0011-self-contained-engine-packaging.md) | Ship a statically linked engine with Clang's builtin headers | Accepted | 2026-11-10 |
| [0012](0012-rust-engine-architecture.md) | Re-architect the native engine in Rust | Accepted | 2026-11-11 |
| [0013](0013-multi-language-complexity-analysis.md) | Multi-Language Complexity Analysis and Pluggable Frontends | Accepted | 2026-11-12 |
| [0014](0014-benchmark-isolation-architecture.md) | Benchmark Isolation Architecture | Accepted | 2026-11-12 |

## Process

1. Copy the structure of an existing ADR into the next free number, `NNNN-short-title.md`.
2. Set the status to **Proposed** and open a pull request (see [GIT_WORKFLOW](../../engineering/GIT_WORKFLOW.md)).
3. When the ADR is accepted, set the status to **Accepted** with the date. Never rewrite the decision of an accepted ADR. To change it, write a new ADR and mark the old one **Superseded by NNNN**.
4. Update [ARCHITECTURE.md](../../../ARCHITECTURE.md) and this index in the same change.

Statuses: Proposed, Accepted, Deprecated, Superseded.
