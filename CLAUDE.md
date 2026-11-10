# CLAUDE.md

Persistent instructions for Claude Code in the BigO Lens repository. The full agent rules are in
[AGENTS.md](AGENTS.md), and the engineering rulebook is [RULES.md](RULES.md). Both are binding.

## Workflow

1. Read [RULES.md](RULES.md) and the architecture docs relevant to the task ([ARCHITECTURE.md](ARCHITECTURE.md),
   [docs/architecture/SYSTEM_DESIGN.md](docs/architecture/SYSTEM_DESIGN.md), and the specification for the area you
   touch in `docs/specifications/`).
2. Inspect the existing code and tests before editing. Avoid unnecessary rewrites; preserve public interfaces and
   protocol compatibility.
3. Make the change within the existing module boundaries (see below).
4. Run the relevant tests and linters. Report the real outcome. Never claim that unrun tests passed.
5. Update documentation, schemas, examples (`scripts/development/regenerate-examples.sh`) and `CHANGELOG.md` when
   behaviour changes.
6. If something is unsupported or cannot be done correctly, report it honestly rather than approximating.

## Repository map

| Path | Contents |
| --- | --- |
| `engine/` | C++20 engine: `bigolens_core` (api, models, complexity, constraints, estimation, feasibility), `bigolens_clang_frontend` (`src/parser/clang_frontend.cpp` — the only Clang-aware file), `bigolens-engine` CLI |
| `benchmark/` | Measurement statistics and growth-model fitting (M4 runner not implemented) |
| `tests/` | Integration tests and C++ regression fixtures; unit tests live in `engine/tests/` and `benchmark/tests/` |
| `shared/` | Protocol JSON schemas and real engine example requests/responses |
| `extension/` | TypeScript VS Code extension (`src/services` are VS Code-free and unit-tested with node:test) |
| `docs/` | Design, architecture, specifications, engineering and user documentation |

## Commands

```bash
cmake --preset dev && cmake --build --preset dev && ctest --preset dev     # native (Clang frontend)
cmake --preset no-clang && cmake --build --preset no-clang && ctest --preset no-clang
cd extension && npm ci && npm run compile && npm run lint && npm test
npm run test:integration    # needs a built engine (build/dev or BIGOLENS_ENGINE_PATH)
npm run validate:schemas
scripts/development/format.sh --check
```

## Hard rules (summary)

- Never invent complexity, workload, runtime, memory, confidence or feasibility values; return unknown/unsupported.
- No regex-based C++ parsing; no Clang types outside the frontend; no Clang dependency in `bigolens_core`.
- Engine stdout carries exactly one JSON response; diagnostics go to stderr.
- No shell-based process spawning, no unvalidated input, no machine-specific paths, no secrets.
- Avoid new dependencies unless clearly necessary.
