# Instructions for AI coding agents

These instructions apply to any AI coding agent working in this repository (Claude Code reads
[CLAUDE.md](CLAUDE.md), which points here). They supplement, and never override, [RULES.md](RULES.md).

## Before changing anything

1. Read [RULES.md](RULES.md) and the documents relevant to the task:
   - architecture and module boundaries: [ARCHITECTURE.md](ARCHITECTURE.md),
     [docs/architecture/SYSTEM_DESIGN.md](docs/architecture/SYSTEM_DESIGN.md)
   - analysis semantics: [docs/specifications/COMPLEXITY_MODEL.md](docs/specifications/COMPLEXITY_MODEL.md),
     [CONSTRAINT_MODEL.md](docs/specifications/CONSTRAINT_MODEL.md),
     [FEASIBILITY_MODEL.md](docs/specifications/FEASIBILITY_MODEL.md)
   - protocol: [docs/specifications/JSON_PROTOCOL.md](docs/specifications/JSON_PROTOCOL.md) and `shared/schemas/`
   - security-sensitive work (process spawning, benchmarking, webviews): [SECURITY.md](SECURITY.md),
     [docs/engineering/THREAT_MODEL.md](docs/engineering/THREAT_MODEL.md)
2. Inspect the existing implementation and its tests before editing. Prefer small, targeted changes over rewrites.
3. Check [docs/development/IMPLEMENTATION_PLAN.md](docs/development/IMPLEMENTATION_PLAN.md) for the issue being worked
   on and its acceptance criteria.

## Non-negotiable rules

- **Never fabricate analysis results.** No invented complexity, workload, runtime, memory, confidence or feasibility.
  If a construct is not supported, the result must be `unknown` or `unsupported`, with the reason in `limitations`.
- Keep bound status (proven / estimated / unknown / unsupported) and confidence separate.
- Never use regular expressions as the primary C++ parser. Use the Clang AST.
- Keep module boundaries:
  - In Rust: `#![forbid(unsafe_code)]` in `crates/bigolens-core`; Clang FFI isolated in `crates/bigolens-clang`; CLI in `crates/bigolens-engine`.
  - In C++ (transitional): only `engine/src/parser/clang_frontend.cpp` may include Clang headers; `bigolens_core` must not depend on Clang.
  - The extension talks to the engine only through the versioned JSON protocol.
- Protocol changes require schema updates, documentation updates and a version decision (see JSON_PROTOCOL.md).
- Validate all external input: requests, constraint files, engine output, settings and environment variables.
- Spawn processes without a shell, with fixed argument lists. Never execute user code outside the planned, isolated
  benchmark subsystem.
- Do not add dependencies without a clear need; justify any new one in the pull request.
- No machine-specific paths, credentials, tokens or secrets in the repository.

## After changing code

- Run the relevant checks and report their actual results:
  - Rust workspace: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`
  - native C++: `cmake --build --preset dev && ctest --preset dev` (or the `no-clang` preset)
  - extension: `cd extension && npm run lint && npm test` (and `npm run test:integration` when the engine is built)
  - formatting: `scripts/development/format.sh --check`
- **Never claim a test passed unless you ran it and saw it pass.** If a check could not be run (missing toolchain,
  no display), say so explicitly.
- Add or update tests for every behavioural change; regression fixtures go in `tests/fixtures/cpp/`.
- Update documentation and `CHANGELOG.md` when behaviour, configuration or the protocol changes.
- If a task cannot be completed correctly within these constraints, stop and explain why instead of producing a
  plausible-looking approximation.
