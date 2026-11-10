# BigO Lens for VS Code

Constraint-aware complexity analysis for C++, aimed at competitive programmers.

BigO Lens parses the active C++ file with Clang, reports the structural cost of each loop nest as a symbolic
expression and an O() bound, evaluates the worst-case workload under the constraints you provide (including
relationships such as "the sum of n over all test cases is at most 2·10⁵"), and estimates static memory.

Every result carries a bound status (proven, estimated, unknown, unsupported) and a separate confidence level.
Constructs the analyzer cannot model are reported as unknown or unsupported rather than guessed.

**Status:** early development (0.1.0). Runtime estimation and benchmarking are not implemented yet. The extension
needs the native `bigolens-engine`, which is built from source; see the
[repository README](https://github.com/chowdhuryrifat/bigo-lens#readme).

## Commands

- **BigO Lens: Analyze Active C++ File**
- **BigO Lens: Edit Constraints for Active File** — creates `<file>.bigolens.json`
- **BigO Lens: Show Engine Capabilities**

## Settings

- `bigoLens.enginePath` — absolute path to `bigolens-engine` (machine setting)
- `bigoLens.cppStandard` — `c++11` … `c++23` (default `c++20`)
- `bigoLens.analysisTimeoutMs` — engine timeout (default 30000)

Licensed under the MIT License.
