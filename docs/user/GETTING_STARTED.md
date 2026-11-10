# Getting Started

BigO Lens analyses a C++ solution in VS Code. It reports, for each loop nest, the **structural count** of loop-body executions as a formula, its **asymptotic class** (for example `O(n^2)`), and the **worst-case workload** under the problem constraints you enter. It also reports static memory estimates and a feasibility verdict against your time and memory limits.

> **Project status: early (0.1.0).** The analysis is deliberately *partial*. It recognizes a defined set of loop shapes and reports everything else as `unknown` or `unsupported` instead of guessing. Time feasibility is currently **always `unknown`**, because runtime measurement (benchmarking) is not implemented yet. See [LIMITATIONS.md](../specifications/LIMITATIONS.md) and the [FAQ](FAQ.md).

## 1. Installation

BigO Lens is **not yet published** on the Visual Studio Marketplace or Open VSX. There are two ways to run it:

- **Install a Linux x64 `.vsix`** (§1.4). The package contains a self-contained engine. Build it yourself, or download
  the `bigo-lens-vsix-linux-x64` artifact of a CI run or a GitHub release asset.
- **Build from source** and run the extension in VS Code's **Extension Development Host** (§1.1–1.3).

**Runtime requirements of the packaged extension (Linux x64):**

- VS Code 1.85 or later.
- glibc from Ubuntu 24.04 or a newer, compatible distribution. The engine is built on Ubuntu 24.04.
- The C++ standard library headers your code includes. Install them with `sudo apt install g++` (or the
  equivalent `libstdc++-dev` package). Without them, files that include `<iostream>`, `<vector>` and similar
  headers fail with `source_parse_error`.
- LLVM or Clang does **not** need to be installed. The engine is statically linked and ships Clang's builtin
  headers ([ADR 0011](../architecture/DECISIONS/0011-self-contained-engine-packaging.md)).

macOS and Windows packages are not available yet.

### 1.1 Prerequisites

| Tool | Version | Notes |
| --- | --- | --- |
| CMake | 3.24 or newer | |
| A C++20 compiler | GCC, Clang or MSVC | |
| Clang/LLVM development libraries | 18 or newer | Needed for source analysis. Linux is the primary platform |
| Ninja | any | Optional, but recommended |
| Node.js | 22 or newer | For building and testing the extension |
| VS Code | 1.85 or newer | |

On Ubuntu:

```bash
sudo apt install cmake ninja-build clang-18 llvm-18-dev libclang-18-dev
# Optional: avoid downloading dependencies at configure time
sudo apt install nlohmann-json3-dev libgtest-dev
```

To check what is installed:

```bash
scripts/setup/check-environment.sh
```

macOS and Windows can build the core engine, but the Clang front end that performs source analysis is **experimental** on those platforms (see [TROUBLESHOOTING.md §9](../development/TROUBLESHOOTING.md#9-platform-notes-macos-and-windows)).

### 1.2 Build the engine

```bash
git clone https://github.com/chowdhuryrifat/bigo-lens.git
cd bigo-lens
cmake --preset dev
cmake --build --preset dev
ctest --preset dev
```

If CMake cannot find Clang, pass its location explicitly:

```bash
cmake --preset dev -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang
```

The engine is built at `build/dev/engine/bigolens-engine`. Check it:

```bash
build/dev/engine/bigolens-engine --version
```

### 1.3 Build and launch the extension

```bash
cd extension
npm ci
npm run compile
```

Then open the **repository root** in VS Code and press **F5**, choosing the **"Run Extension"** launch configuration from `.vscode/launch.json`. A second VS Code window opens: the Extension Development Host, with BigO Lens loaded. When running from the repository, the extension finds the development build in `build/dev` automatically.

To use an engine from somewhere else, set `bigoLens.enginePath` to its absolute path (see [CONFIGURATION.md](CONFIGURATION.md#engine-location)).

### 1.4 Build and install a self-contained `.vsix` (Linux x64)

You need the Clang/LLVM development packages, including the static libraries (`llvm-<v>-dev`,
`libclang-<v>-dev`):

```bash
scripts/build/package-extension.sh -G Ninja     # add -DClang_DIR=... if Clang is not found automatically
scripts/test/verify-vsix.sh                     # optional: contents, clean-container and VS Code checks (needs Docker)
```

The script builds the engine with the `package` preset (Release, statically linked Clang), runs the native tests,
copies the stripped engine and Clang's builtin headers to `extension/bin/`, and writes `extension/bigo-lens.vsix`.

Install the package with **Extensions → … → Install from VSIX…**, or run `code --install-extension
extension/bigo-lens.vsix`. The bundled engine is found automatically; leave `bigoLens.enginePath` empty.

## 2. Your first analysis

1. In the Extension Development Host, open a C++ file. For example, copy `tests/fixtures/cpp/09_sum_constrained_test_cases.cpp` into a scratch folder:

   ```cpp
   #include <iostream>

   int main() {
     int t = 0;
     std::cin >> t;
     while (t--) {
       int n = 0;
       std::cin >> n;
       long long acc = 0;
       for (int i = 0; i < n; ++i) {
         for (int j = 0; j < n; ++j) {
           acc += i * j;
         }
       }
       std::cout << acc << '\n';
     }
   }
   ```

2. Run **BigO Lens: Edit Constraints for Active File** from the Command Palette. This creates `<file>.bigolens.json` next to your source file, from a template, and opens it. The template already describes a classic multi-test constraint:

   ```json
   {
     "variables": {
       "t": { "min": 1, "max": 10 },
       "n": { "min": 1, "max": 200000 }
     },
     "sum_constraints": [{ "variable": "n", "count_variable": "t", "max_total": 200000 }],
     "time_limit_ms": 2000,
     "memory_limit_mb": 256
   }
   ```

   Constraint variables are matched **by name** to identifiers in your code. Here, `n` binds to the loop bound `n`, and `t` to the `while (t--)` counter.

3. Run **BigO Lens: Analyze Active C++ File**. You can also use the play button in the **BigO Lens** view title.

4. Open the **BigO Lens** activity-bar container and look at **Analysis Results**. You should see one loop-nest finding with:
   - **pattern:** `test-case-loop`
   - **O():** the asymptotic class of the structural count
   - **workload:** 4·10^10, with method `sum-constraint-packing`. Because Σn ≤ 2·10^5, the worst case is a single test with n = 2·10^5, giving (2·10^5)² = 4·10^10. Multiplying the independent maxima would give t·n² = 4·10^11, which is ten times too pessimistic.
   - **bound status** and **confidence:** how strong the claim is, and how much to trust it
   - **memory** and **feasibility** summaries. Time feasibility shows `unknown`, because no runtime estimate exists yet.

5. Click a finding to jump to its loop. Each finding also appears as an Information-level entry in the **Problems** panel.

The analysis uses the editor's **current contents**, including unsaved changes.

## 3. Reading the results

| Field | Meaning |
| --- | --- |
| Expression | The structural count of innermost loop-body executions. This is **not** a CPU instruction count or a runtime |
| O() | The asymptotic upper bound derived from the expression, assuming the variables grow without bound |
| Workload | The expression evaluated at the worst case allowed by your constraints, with the method used |
| Bound status | `proven` (derived exactly under the stated assumptions), `estimated` (heuristic step involved), `unknown` (could not be determined), `unsupported` (construct not handled) |
| Confidence | `high`, `medium`, `low` or `none`. This is separate from the bound status, and reflects assumptions and limitations |
| Assumptions and limitations | What the result relies on, and what it leaves out. For example, the cost of called functions is not included |

The full model is described in [ANALYSIS_MODEL.md](../design/ANALYSIS_MODEL.md) and [COMPLEXITY_MODEL.md](../specifications/COMPLEXITY_MODEL.md).

## 4. Other commands

- **BigO Lens: Show Engine Capabilities** shows the engine version, the protocol version, the front end (the Clang version, or none), and the maturity of each analysis feature.

## 5. Next steps

- [CONFIGURATION.md](CONFIGURATION.md): settings and the full constraints file format
- [FAQ.md](FAQ.md): why a result is `unknown`, what is and is not measured
- [TROUBLESHOOTING.md](../development/TROUBLESHOOTING.md): build and runtime problems
- [CONTRIBUTING.md](../../CONTRIBUTING.md): help improve BigO Lens
