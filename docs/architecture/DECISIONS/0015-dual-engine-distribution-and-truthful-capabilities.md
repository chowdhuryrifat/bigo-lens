# 0015 Dual Engine Distribution, Explicit Engine Naming, and Truthful Capabilities

- **Status:** Accepted
- **Date:** 2026-11-16
- **Milestone:** Cross-Engine Integration & Release Hardening (BL-506, BL-603)
- **Deciders:** BigO Lens Core Contributors

## Context

The repository contains two engine implementations sharing the JSON protocol:
1. **C++ Engine (`engine/`):** Based on Clang LibTooling (`kEngineName = "bigolens-clang-engine"`). Performs deep C++ AST static analysis, recurrence extraction, and exact global/static memory bounds. Dynamic benchmarking is not implemented (`benchmark: not_implemented`). Historically packaged in the Linux VSIX.
2. **Rust Engine (`crates/bigolens-*`):** Based on tree-sitter AST parsers and an empirical benchmark runner (`ENGINE_NAME = "bigolens-rust-engine"`). Analyzes C++, Python, Java, Rust, and JavaScript, and implements Linux namespace/seccomp benchmarking isolation. Historically packaged in Windows and macOS VSIX.

An audit revealed several critical discrepancies:
- Both engines previously reported the identical name `"bigolens-engine"` in response metadata.
- Capabilities were not truthful: the C++ engine reported supported actions that did not match its implementation, and the Rust engine's capability matrix did not reflect its actual platform isolation capabilities.
- On Windows and macOS, the Rust engine failed closed with `isolation_unavailable` for benchmarks, while the extension offered no way to run unsandboxed with explicit consent.
- CI only verified the Linux VSIX artifact with `verify-vsix.sh`, leaving Windows and macOS packages untested before release.

## Decision

We adopt **Option B** (distinct engine names, truthful capabilities, multi-platform CI verification, and extension unsandboxed fallback):

### 1. Distinct Engine Names
- The C++ Clang-based engine identifies as `engine.name: "bigolens-clang-engine"`.
- The Rust multi-language engine identifies as `engine.name: "bigolens-rust-engine"`.
- Both engines continue to implement the versioned JSON protocol (`protocol_version: "0.1.0"`).

### 2. Truthful Capabilities
- Each engine reports an exact, non-inflated capability matrix in response to `action: "capabilities"`.
- `supported_actions`, `supported_languages`, and `benchmark.isolation_levels` report reality per platform.
- On platforms without kernel namespaces (Windows, macOS), the Rust engine reports `isolation_levels: ["process-limits-only"]` with `isolation_available: false`.

### 3. CI Verification for All Shipped Artifacts
- Add `package-windows` and `package-macos` jobs in `.github/workflows/ci.yml`.
- Adapt `scripts/test/verify-vsix.sh` to work cross-platform across both C++ and Rust bundled engines without requiring system Clang development packages.
- Ensure every shipped `.vsix` is extracted, smoke-tested, and verified against protocol schemas in CI.

### 4. Extension Graceful Fallback for Non-Sandboxed Execution
- When the benchmark engine responds with error code `isolation_unavailable`, the extension displays a modal confirmation dialog warning the user that genuine sandbox isolation is unavailable on their operating system.
- If the user explicitly accepts the risk, the extension retries the benchmark with `allow_unsandboxed: true`.
- The engine stamps the benchmark results with `isolated: false` and emits warning messages in `warnings`.

## Consequences

- Clients and users can distinguish the active engine backend unambiguously.
- No false claims of sandbox isolation or unsupported features are made.
- Windows and macOS release packages are validated with the same rigor as Linux packages in continuous integration.
