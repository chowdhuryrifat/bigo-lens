# ADR 0002: Run the analysis in a separate native engine

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

Clang LibTooling is a C++ library (see [ADR 0001](0001-clang-ast-instead-of-regex.md)). The editor integration runs in the VS Code extension host, which is Node.js. The analysis must be:

- Testable on its own and usable from a CLI and from CI.
- Unable to crash or hang the editor.
- Buildable without Clang, for platforms where the frontend is not yet supported.

## Decision

Build a standalone C++20 executable, **`bigolens-engine`**, made of layered static libraries:

- **`bigolens_core`**: `api`, `models`, `complexity`, `constraints`, `estimation`, `feasibility` and `analyzers`. It has **no Clang dependency** and links only nlohmann/json.
- **`bigolens_clang_frontend`**: optional, controlled by `BIGOLENS_ENABLE_CLANG`. It contains only `engine/src/parser/clang_frontend.cpp`.
- **`bigolens-engine`**: `engine/src/main.cpp`. It injects the analyzer into `api::RequestHandler` as a non-owning pointer. With Clang disabled the pointer is null, and `analyze` returns `frontend_unavailable`.

The extension talks to the engine only through the JSON protocol ([ADR 0004](0004-json-ipc.md)).

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Node native addon (N-API) linking Clang | A crash or hang takes down the extension host. ABI coupling to Electron and Node versions. Hard to test outside the editor. |
| WebAssembly build of Clang | Very large binary. Reading system headers from WASM is awkward. Slow to start. |
| Engine in TypeScript | No semantic C++ frontend is available. |
| One monolithic library including Clang | The core could not be built or tested on Windows or macOS CI without LLVM. |

## Consequences

**Positive**

- Process isolation. Engine failures become structured errors or a killed process, never an extension crash.
- The core is unit-tested on Linux, macOS and Windows without LLVM.
- The engine works on its own: `bigolens-engine --request file.json`.
- A clear Clang boundary makes it possible to replace or add frontends.

**Negative**

- The engine binary must be distributed per platform (bundled in `<extension>/bin/`) or located by the user.
- Each request pays the cost of starting a process and initialising Clang.
- Two languages and two toolchains to maintain.

## Follow-up considerations

- Packaging of per-platform engine binaries in the `.vsix`. The release workflow currently packages the Linux engine.
- Revisit process start-up cost if interactive latency becomes a problem (see [ADR 0004](0004-json-ipc.md)).
