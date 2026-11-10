# Troubleshooting

This document covers common build, test and runtime problems, and how to fix them. If your problem is not listed, run `scripts/setup/check-environment.sh` (it reports which tools are available) and open a bug report with its output.

- [1. Clang is not found at configure time](#1-clang-is-not-found-at-configure-time)
- [2. `stddef.h` not found or missing resource directory](#2-stddefh-not-found-or-missing-resource-directory)
- [3. Link errors mentioning `typeinfo for clang::…`](#3-link-errors-mentioning-typeinfo-for-clang)
- [4. FetchContent download failures (offline or proxy)](#4-fetchcontent-download-failures-offline-or-proxy)
- [5. The extension cannot find the engine](#5-the-extension-cannot-find-the-engine)
- [6. `frontend_unavailable` error](#6-frontend_unavailable-error)
- [7. Other engine errors](#7-other-engine-errors)
- [8. Extension test problems](#8-extension-test-problems)
- [9. Platform notes: macOS and Windows](#9-platform-notes-macos-and-windows)

---

## 1. Clang is not found at configure time

**Symptom**

```text
CMake Error at cmake/ClangTooling.cmake:5 (find_package):
  Could not find a package configuration file provided by "Clang" ...
```

**Cause.** `BIGOLENS_ENABLE_CLANG` is `ON` (the `dev`, `release` and `ci` presets), but CMake cannot locate `ClangConfig.cmake`.

**Fix**

1. Install the Clang development packages. On Ubuntu:

   ```bash
   sudo apt install cmake ninja-build clang-18 llvm-18-dev libclang-18-dev
   ```

   Newer LLVM versions also work. Replace `18` with the version you install.
2. Point CMake at the package configuration directory:

   ```bash
   cmake --preset dev -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang
   ```

   If LLVM is also not found, add `-DLLVM_DIR=/usr/lib/llvm-18/lib/cmake/llvm`.
3. To find the right directory on your system:

   ```bash
   find / -name ClangConfig.cmake 2>/dev/null
   ```

4. If you only need the core engine (protocol, constraints, workload, memory and feasibility, without source analysis), use the preset that does not need Clang:

   ```bash
   cmake --preset no-clang && cmake --build --preset no-clang && ctest --preset no-clang
   ```

**Note.** CMake caches the result of the first configure. After installing packages, or changing `Clang_DIR`, delete the build directory (`rm -rf build/dev`) or pass `--fresh`.

## 2. `stddef.h` not found or missing resource directory

**Symptom.** Analysis of a file that includes standard headers fails with `source_parse_error` and diagnostics such as:

```text
fatal error: 'stddef.h' file not found
```

You may also see this warning at configure time:

```text
CMake Warning: Could not determine the Clang resource directory; set BIGOLENS_CLANG_RESOURCE_DIR
```

**Cause.** When Clang is used as a library, it needs its *resource directory*, which contains builtin headers such as `stddef.h` and `stdarg.h`. CMake detects it at configure time by running `clang -print-resource-dir` from the matching LLVM installation, or by falling back to `<llvm>/lib/clang/<major>`. If no matching `clang` binary is installed, detection fails.

**Fix.** Choose one of the following:

- Install the matching `clang-<version>` package (not only `libclang-<version>-dev`) and re-configure with `--fresh`.
- Set the directory at configure time:

  ```bash
  cmake --preset dev -DBIGOLENS_CLANG_RESOURCE_DIR="$(clang-18 -print-resource-dir)"
  ```

- Override it at runtime without rebuilding:

  ```bash
  export BIGOLENS_CLANG_RESOURCE_DIR="$(clang-18 -print-resource-dir)"
  ```

  The environment variable takes precedence over the build-time default. If you start VS Code from a desktop launcher, set the variable in a place VS Code inherits, or launch VS Code from a shell that has it set.

The resource directory must come from the **same LLVM major version** that the engine was linked against.

## 3. Link errors mentioning `typeinfo for clang::…`

**Symptom**

```text
undefined reference to `typeinfo for clang::ASTConsumer'
undefined reference to `typeinfo for clang::RecursiveASTVisitor<...>'
```

**Cause.** Distribution LLVM packages are built **without RTTI**. A translation unit that subclasses Clang types must also be compiled with `-fno-rtti`. `engine/CMakeLists.txt` adds `-fno-rtti` to `bigolens_clang_frontend` automatically when `LLVM_ENABLE_RTTI` is off. The error appears when:

- code that subclasses Clang types was added outside `engine/src/parser/clang_frontend.cpp`, in a target compiled with RTTI, or
- `dynamic_cast` or `typeid` was used on Clang types, or
- LLVM's CMake configuration reports the wrong RTTI setting, as can happen with a custom LLVM build.

**Fix**

- Keep all Clang-dependent code in `clang_frontend.cpp`. This is required by [RULES.md §1.1](../../RULES.md#11-language-and-build).
- Use `llvm::isa`, `llvm::dyn_cast` and `llvm::dyn_cast_or_null` instead of `dynamic_cast`.
- Check the setting with `llvm-config-18 --has-rtti`. If it disagrees with `LLVM_ENABLE_RTTI` in `LLVMConfig.cmake`, fix your LLVM installation or use the distribution packages.

## 4. FetchContent download failures (offline or proxy)

**Symptom.** Configure fails while downloading `json.tar.xz` or `googletest-1.15.2.tar.gz`, or reports a hash mismatch.

**Cause.** nlohmann/json and GoogleTest are first looked up with `find_package`. If they are not installed, CMake downloads pinned release archives from GitHub and verifies their SHA-256 hashes. Offline machines, proxies and captive portals break the download. A hash mismatch means that the downloaded file is not the expected archive.

**Fix**

- Install the dependencies from your distribution, so no download is needed:

  ```bash
  sudo apt install nlohmann-json3-dev libgtest-dev
  ```

  The build accepts nlohmann/json 3.11 or newer and GoogleTest 1.14 or newer, as found by `find_package`. Re-configure with `--fresh` afterwards.
- Behind a proxy, export `https_proxy` before running CMake.
- **Never** edit the `URL_HASH` to make a mismatch go away. A mismatch is a security signal. Report it if it persists from a clean network.
- If you do not need tests, `-DBIGOLENS_BUILD_TESTS=OFF` skips GoogleTest entirely.

## 5. The extension cannot find the engine

**Symptom.** "Analyze Active C++ File" reports that `bigolens-engine` could not be found, or the "BigO Lens" output channel shows the locations that were searched.

**Cause.** The extension looks for the engine in this order:

1. The `bigoLens.enginePath` setting
2. The `BIGOLENS_ENGINE_PATH` environment variable
3. A bundled `<extension>/bin/bigolens-engine` (`.exe` on Windows). Release packages do **not** bundle the engine yet (see [IMPLEMENTATION_PLAN.md, BL-108](IMPLEMENTATION_PLAN.md#bl-108-bundle-the-engine-binary-into-the-vsix-linux)).
4. A development build in the repository: `build/<dev|release|ci|no-clang>/engine/[Debug/|Release/]bigolens-engine[.exe]`

**Fix**

- Build the engine first: `cmake --preset dev && cmake --build --preset dev`.
- Or set the path explicitly to an **absolute** path in your user settings:

  ```json
  { "bigoLens.enginePath": "/absolute/path/to/build/dev/engine/bigolens-engine" }
  ```

- A **relative** `bigoLens.enginePath` is rejected rather than guessed. And if `bigoLens.enginePath` is set but unusable (a missing or non-executable file), the extension reports that path and does **not** fall back to the other locations. Clear the setting to use the automatic search.
- Check that the file is executable (`chmod +x`) and runs: `bigolens-engine --version`.
- The environment variable is read from the environment VS Code was started with. Restart VS Code after changing it.
- The development-build fallback (step 4) works when the extension runs from the repository, for example in the Extension Development Host. It does not apply to an installed `.vsix`.

## 6. `frontend_unavailable` error

**Symptom.** `capabilities` works, but `analyze` returns:

```json
{ "code": "frontend_unavailable", "message": "..." }
```

and `engine.frontend` is `null`.

**Cause.** The engine was built with `BIGOLENS_ENABLE_CLANG=OFF`, for example with the `no-clang` preset. This is how the macOS and Windows CI builds are made. Without the Clang front end, the engine cannot parse source code.

**Fix.** Rebuild with the Clang front end (the `dev` or `release` preset, with Clang development libraries installed; see §1). Then point the extension at that build. If both `build/dev` and `build/no-clang` exist, set `bigoLens.enginePath` explicitly, so you know which one is used.

## 7. Other engine errors

| Error code | Meaning | What to do |
| --- | --- | --- |
| `source_parse_error` | Clang reported errors. The diagnostics list them (at most 50), and analysis is not run on code with errors | Fix the compile errors. Check `bigoLens.cppStandard`. If the errors are about builtin headers, see §2 |
| `invalid_request` | The request or the constraints failed validation. `path` is a JSON pointer, such as `/constraints/variables/n` | Fix the sidecar file at that path. Common causes: `min` greater than `max`, a sum constraint referring to undeclared variables, values above 10^18, unknown fields |
| `request_too_large` | The request exceeds 4 MiB | The source limit is 2 MiB. Analyse a smaller file |
| `malformed_json` | The request is not valid JSON, or is nested deeper than 64 levels | Usually a hand-written request. Validate it |
| `unsupported_schema_version` | The client and engine protocol versions are incompatible | Use an extension and engine from the same release or commit |
| `not_implemented` | The `benchmark` action, which does not exist yet | Expected. See [ROADMAP.md, M4](ROADMAP.md#m4-benchmarking) |
| `internal_error` | A bug in the engine | Open a bug report with the source (if you can share it) and the engine's stderr from the output channel |

**The analysis times out.** The default `bigoLens.analysisTimeoutMs` is 30000. The first analysis of a file that includes `<bits/stdc++.h>` with a Debug engine can be slow. Use a Release build (`cmake --preset release && cmake --build --preset release`) or raise the timeout.

## 8. Extension test problems

- **`npm ci` fails with an engine-strict error.** Node 20 or newer is required, because `extension/.npmrc` sets `engine-strict=true`.
- **`npm run test:integration` reports "skipped".** No engine was found. Build `build/dev`, or set `BIGOLENS_ENGINE_PATH`.
- **`npm run test:vscode` fails on headless Linux** with display errors. Run it under xvfb: `xvfb-run -a npm run test:vscode`. The first run downloads VS Code into `extension/.vscode-test/`, which needs network access.
- **Schema errors after changing `shared/schemas/`.** `npm run compile` copies the schemas into `extension/schemas/`. Re-run it, and never edit the copies.

## 9. Platform notes: macOS and Windows

**Support level** (from CI configuration; see [LIMITATIONS.md](../specifications/LIMITATIONS.md#15-platform-support)):

| Platform | Core engine and tests | Clang front end |
| --- | --- | --- |
| Linux (Ubuntu, LLVM 18 in CI) | Supported | Supported (primary platform) |
| macOS | Supported (CI, without Clang) | **Experimental** (Homebrew LLVM; the CI job is allowed to fail) |
| Windows (MSVC) | Supported (CI, `BIGOLENS_ENABLE_CLANG=OFF`) | **Experimental, untested** |

### macOS

- Apple's Xcode Clang does **not** ship LibTooling's CMake packages. Use Homebrew LLVM:

  ```bash
  brew install llvm cmake ninja
  cmake --preset dev -DClang_DIR="$(brew --prefix llvm)/lib/cmake/clang"
  ```

- The resource directory: `export BIGOLENS_CLANG_RESOURCE_DIR="$("$(brew --prefix llvm)/bin/clang" -print-resource-dir)"`.
- Standard library headers may require the SDK path. If analysis reports missing system headers, this is a known gap of the experimental macOS front end. Please report it with your macOS and LLVM versions.
- If you do not need the front end, use the `no-clang` preset.

### Windows

- Use a "Developer PowerShell for VS" (or run `vcvarsall.bat`), so that MSVC, CMake and Ninja are on `PATH`.
- Supported configuration: `cmake --preset no-clang`, then `cmake --build --preset no-clang`, then `ctest --preset no-clang`.
- Multi-config generators (Visual Studio) place the binary at `build\no-clang\engine\Debug\bigolens-engine.exe`. The extension's development-build lookup checks the `Debug\` and `Release\` subdirectories.
- Building the Clang front end on Windows requires an LLVM installation that includes the CMake packages and development libraries. This is untested. Contributions are welcome via [BL-602](IMPLEMENTATION_PLAN.md#bl-602-graduate-the-clang-front-end-on-macos-and-windows).
- Line endings: the repository uses LF (`.gitattributes`). If Git converts files to CRLF, clang-format checks may fail. Run `git config core.autocrlf false` and re-checkout.

## macOS: frontend reports errors such as `'_Tp' does not refer to a value`

Observed in the experimental CI job (`macos-latest`, Homebrew `llvm@18`): the engine builds, but when it parses
sources that include standard headers, the Clang 18 frontend picks up the system SDK's libc++ headers, which use
builtins that Clang 18 does not know. Every analysis then fails with `source_parse_error`, and 14 of 93 CTest tests
fail. This is why the macOS Clang frontend is marked experimental. Likely fixes (not yet verified): build against a
Homebrew LLVM whose version matches or exceeds the SDK's libc++ expectations, or point the frontend at the
matching LLVM's libc++ headers. Tracked as part of cross-platform validation in
[IMPLEMENTATION_PLAN.md](IMPLEMENTATION_PLAN.md).
