# Contributing to BigO Lens

Thank you for helping improve BigO Lens. This guide covers setting up a development environment, building, testing, and getting a change merged. Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).

Before you write code, read [RULES.md](RULES.md). Its first rule is the most important: **BigO Lens never reports a result it cannot justify.** An honest `unknown` is always better than a plausible guess.

## 1. Development environment

### Required

| Tool | Version | Used for |
| --- | --- | --- |
| Git | any recent | |
| CMake | 3.24 or newer | Engine build (presets) |
| C++20 compiler | GCC, Clang or MSVC | Engine |
| Clang/LLVM development libraries | 18 or newer | Clang front end (Linux is the primary platform) |
| Node.js | 22 or newer | Extension (`engine-strict` is enabled) |
| VS Code | 1.85 or newer | Running the extension |

### Recommended

Ninja, clang-format, clang-tidy, and `xvfb` on headless Linux (for VS Code smoke tests).

### Ubuntu

```bash
sudo apt install cmake ninja-build clang-18 llvm-18-dev libclang-18-dev
# Optional: use system packages instead of downloading at configure time
sudo apt install nlohmann-json3-dev libgtest-dev
# Optional: formatting and linting tools, plus a headless display for VS Code tests
sudo apt install clang-format clang-tidy xvfb
```

Newer LLVM versions also work. CI uses LLVM 18.

### Check your environment

```bash
scripts/setup/check-environment.sh
```

The script reports which tools are available and which are missing. It does not install anything.

macOS and Windows can build and test the core engine, but the Clang front end is experimental on those platforms. See [TROUBLESHOOTING.md §9](docs/development/TROUBLESHOOTING.md#9-platform-notes-macos-and-windows).

## 2. Building

### Engine

```bash
cmake --preset dev            # Debug, with the Clang front end
cmake --build --preset dev
```

If Clang is not found automatically:

```bash
cmake --preset dev -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang
```

To build without Clang (the core engine only, on any platform):

```bash
cmake --preset no-clang && cmake --build --preset no-clang
```

| Preset | Build type | Clang front end | Warnings as errors |
| --- | --- | --- | --- |
| `dev` | Debug | on | off |
| `release` | Release | on | off |
| `no-clang` | Debug | off | off |
| `ci` | Release | on | **on** |

Build output goes to `build/<preset>/`. The engine executable is `build/<preset>/engine/bigolens-engine`.

### Extension

```bash
cd extension
npm ci
npm run compile     # copies shared/schemas into extension/schemas, then runs tsc
```

To run it, open the repository root in VS Code and press **F5** with the **"Run Extension"** configuration. The Extension Development Host finds the engine in `build/dev` automatically.

### Everything

```bash
scripts/build/build-all.sh
```

## 3. Running tests

```bash
# Engine: unit and integration tests
ctest --preset dev
ctest --preset dev -L unit           # unit only
ctest --preset dev -L integration    # integration only
ctest --preset no-clang              # without Clang

# Extension
cd extension
npm test                     # unit tests (node:test)
npm run test:integration     # against a real engine (build/dev, or BIGOLENS_ENGINE_PATH)
npm run test:vscode          # VS Code smoke test; downloads VS Code; use xvfb-run -a on headless Linux
npm run validate:schemas     # schemas and examples

# Everything the scripts can run on this machine
scripts/test/run-all-tests.sh
```

If you changed engine output, regenerate the protocol examples **by running the engine**. Never edit them by hand:

```bash
scripts/development/regenerate-examples.sh
```

The test layers, fixtures and negative-test matrix are described in [TESTING_STRATEGY.md](docs/engineering/TESTING_STRATEGY.md).

## 4. Formatting and linting

| Tool | What it checks | Command |
| --- | --- | --- |
| clang-format (`.clang-format`, Google-based, 120 columns) | C++ formatting | `scripts/development/format.sh` |
| Prettier (`.prettierrc`) | TS, JS, JSON, YAML formatting | `scripts/development/format.sh`, or `npm run format:check` in `extension/` |
| clang-tidy (`.clang-tidy`, every enabled check is an error) | C++ static checks | `clang-tidy -p build/dev <file.cpp>` |
| ESLint (`extension/eslint.config.mjs`) | TypeScript lint | `npm run lint` in `extension/` |
| EditorConfig (`.editorconfig`) | Whitespace and line endings | your editor |

`scripts/development/format.sh` runs clang-format and Prettier over the repository. Run it before every commit. CI fails on unformatted code. Details and examples are in [CODING_STANDARDS.md](docs/engineering/CODING_STANDARDS.md).

## 5. Branches

Branch from an up-to-date `main`:

| Prefix | Use |
| --- | --- |
| `feat/` | New capability |
| `fix/` | Bug fix |
| `docs/` | Documentation only |
| `chore/` | Maintenance, dependencies, tooling |
| `refactor/`, `test/`, `ci/`, `perf/` | As the names suggest |

Use lowercase kebab-case, and include the issue ID when there is one: `feat/bl-203-master-theorem`, `fix/sum-constraint-remainder`. See [GIT_WORKFLOW.md](docs/engineering/GIT_WORKFLOW.md).

## 6. Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
fix(constraints): handle remainder case in sum-constraint packing

The last partial test case was evaluated at n_max instead of the
remaining budget, overstating the bound.

Closes #42
```

- Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`, `revert`.
- Scopes: `engine`, `frontend`, `protocol`, `constraints`, `complexity`, `estimation`, `feasibility`, `benchmark`, `extension`, `build`, `ci`, `docs`.
- Write the summary in the imperative mood, in at most 72 characters, with no trailing period.
- For breaking protocol changes, use `!` and a `BREAKING CHANGE:` footer.
- `Signed-off-by` (`git commit -s`) is welcome, but not required.

## 7. Pull requests

1. **Discuss first.** Open or comment on an issue for anything beyond a small fix, so the scope and acceptance criteria are agreed. The backlog with ready-to-pick issues is in [IMPLEMENTATION_PLAN.md](docs/development/IMPLEMENTATION_PLAN.md).
2. **Keep it focused.** One logical change per pull request.
3. **Title** in Conventional Commits form, because it becomes the squash commit.
4. **Description:**
   - What changed, and why
   - The linked issue (`Closes #…` or `Refs BL-…`)
   - **How you verified it:** the exact commands you ran and their results. Only list what you actually ran
   - Any change in analysis results for existing fixtures
   - Protocol impact (none, minor or major)
   - Screenshots for UI changes
5. **Checks:** CI must be green, except the experimental macOS Clang job.
6. **Definition of Done:** your change meets [DEFINITION_OF_DONE.md](docs/development/DEFINITION_OF_DONE.md). That means tests, docs, schemas and examples are updated together.
7. **Review:** at least one approval from someone else. Address the feedback with new commits, and do not force-push during review unless asked. The pull request is squash-merged.

### Good first contributions

- New fixtures and near-miss tests for supported loop forms ([BL-201](docs/development/IMPLEMENTATION_PLAN.md#bl-201-fixture-coverage-audit-with-near-miss-negatives))
- Troubleshooting entries for problems you hit while setting up
- macOS and Windows build fixes for the experimental Clang front end

## 8. Reporting issues

Use the GitHub issue templates:

- **Bug report:** what you did, what you expected, and what happened. Include the BigO Lens version (`bigolens-engine --version`), your OS, your LLVM version, a **minimal** C++ file and constraints file that reproduce the problem (if you can share them), and the relevant lines from the **BigO Lens** output channel. Wrong analysis results are bugs. Please say which field is wrong and why.
- **Feature request:** the problem you want solved, with an example program and the result you would expect. Check [ROADMAP.md](docs/development/ROADMAP.md) and [LIMITATIONS.md](docs/specifications/LIMITATIONS.md) first, because it may be planned already.
- **Documentation:** the page, what is wrong or missing, and a suggested fix.

**Do not report security vulnerabilities in public issues.** Follow [SECURITY.md](SECURITY.md).

## 9. Licence

BigO Lens is released under the MIT License. By contributing, you agree that your contributions are licensed under the same terms.
