# BigO Lens — repository manifest

Overview of the major contents of the BigO Lens project foundation (v0.1.0). Generated build trees,
`node_modules/`, compiled extension output and caches are intentionally excluded from the repository and from
backups.

| Path | Contents |
| --- | --- |
| `README.md`, `DESIGN.md`, `ARCHITECTURE.md`, `RULES.md` | Overview, product design, architecture, engineering rulebook |
| `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `LICENSE` | Project policies; MIT licence |
| `CLAUDE.md`, `AGENTS.md` | Instructions for AI coding agents |
| `CMakeLists.txt`, `CMakePresets.json`, `cmake/` | Native build: options, presets (`dev`, `release`, `no-clang`, `ci`), Clang discovery, pinned dependencies, warning policy |
| `engine/include/bigolens/`, `engine/src/` | C++20 engine: protocol (`api`), models, symbolic expressions (`complexity`), constraints and workload, memory and STL cost model (`estimation`), feasibility, analyzer interface, Clang frontend (`parser`), CLI (`main.cpp`) |
| `engine/tests/` | GoogleTest unit tests for every engine module |
| `benchmark/` | Benchmark subsystem: statistics and growth-model fitting library with tests (runner planned for M4) |
| `tests/integration/`, `tests/fixtures/cpp/`, `tests/CMakeLists.txt` | Source → AST → analysis → constraints → JSON integration tests; regression and negative C++ fixtures; engine CLI tests |
| `shared/schemas/` | JSON Schemas for protocol 1.0 requests and responses |
| `shared/examples/` | Example requests and the responses the engine produced for them |
| `extension/` | TypeScript VS Code extension: `src/` (commands, providers, services, models), `test/` (unit, integration, e2e), `package.json`, `tsconfig.json`, ESLint config, asset script |
| `scripts/` | `setup/check-environment.sh`, `build/build-all.sh`, `test/run-all-tests.sh`, `development/regenerate-examples.sh`, `development/format.sh` |
| `.github/` | CI, code-quality, security and release workflows; issue and PR templates; CODEOWNERS; Dependabot |
| `.vscode/` | Recommended extensions, launch configurations (Run Extension), tasks, workspace settings |
| `docs/architecture/` | System design, component design, data flow, ten architecture decision records (`DECISIONS/`) |
| `docs/design/` | UI/UX design with wireframes; analysis model |
| `docs/specifications/` | Requirements, complexity, constraint, feasibility and benchmark models, JSON protocol, limitations |
| `docs/engineering/` | Engineering rules, coding standards, Git workflow, testing strategy, threat model, performance |
| `docs/development/` | Roadmap, implementation plan with GitHub-ready issue backlog, definition of done, troubleshooting |
| `docs/user/` | Getting started, configuration, FAQ |
| `.clang-format`, `.clang-tidy`, `.editorconfig`, `.prettierrc`, `.prettierignore`, `.gitignore`, `.gitattributes`, `.npmrc` | Tooling configuration |
