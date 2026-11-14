# 0014: Evaluation of Header Parsing Cost and Precompiled Preamble Caching

- **Status:** Accepted
- **Date:** 2026-11-14
- **Related issue:** BL-607
- **Deciders:** BigO Lens Core Contributors

## Context

In BigO Lens, request latency on C++ translation units is dominated by AST generation rather than asymptotic complexity analysis. As noted in [PERFORMANCE.md §2](../../engineering/PERFORMANCE.md#2-where-time-goes) and [§5](../../engineering/PERFORMANCE.md#5-measurement-approach), an `analyze` request executes sequentially:

1. Process initialization and JSON request validation;
2. Clang syntax-only compilation (`-fsyntax-only`);
3. AST traversal and symbolic constraint extraction;
4. Asymptotic workload solving and recurrence evaluation;
5. JSON response serialization.

Competitive programming solutions (e.g. from Codeforces, LeetCode, or AtCoder) typically consist of 30–100 lines of user algorithmic logic preceded by heavy standard library includes (`<iostream>`, `<vector>`, `<algorithm>`, `<set>`, `<map>`, `<queue>`, `<cmath>`), or monolithic umbrella headers such as `<bits/stdc++.h>`. When Clang parses `#include <iostream>` or `<vector>`, it expands template headers, inline definitions, and standard library headers totaling tens of thousands of lines of code.

As a result, a 30-line solution can incur 300–500 ms of parsing time cold. To provide interactive editor experiences (such as analysis on document save as designed in BL-505 and defined in [PERFORMANCE.md §4](../../engineering/PERFORMANCE.md#4-budgets-targets)), we must reduce this front-end parsing cost without compromising the prime directive of honesty and analysis determinism ([RULES.md §0](../../../RULES.md#0-the-prime-directive-honesty-of-results)).

## Findings & Empirical Data

To quantify the exact cost of standard headers and evaluate the benefits of precompiled preambles, we developed the empirical benchmarking tool [`scripts/benchmark/measure-header-parse.py`](../../../scripts/benchmark/measure-header-parse.py).

The benchmark evaluates five representative suites comparing cold AST parsing against Clang Precompiled Header (PCH) compilation (`-x c++-header -emit-pch`) and warm PCH-accelerated AST parsing (`-include-pch`):

1. **Minimal / baseline:** No standard library headers (`int main() { return 0; }`), measuring raw compiler process startup and AST initialization overhead.
2. **Algorithms suite:** `<algorithm>`, `<numeric>`.
3. **Containers suite:** `<vector>`, `<set>`, `<map>`, `<string>`.
4. **I/O streams suite:** `<iostream>`, `<sstream>`.
5. **Full competitive programming bundle:** `<iostream>`, `<vector>`, `<algorithm>`, `<set>`, `<map>`, `<queue>`, `<cmath>`.

### Empirical Measurements

Collected on Linux x86_64 with Clang 20 (`Ubuntu clang version 20.1.8`, `-std=c++20`, 5 runs per suite reporting median and median absolute deviation):

| Suite | Headers | Cold Parse (ms) | PCH Compile (ms) | Warm Parse (ms) | Speedup | PCH Size |
|---|---|---|---|---|---|---|
| **Minimal / baseline** | *none* | 37.0 ± 1.7 | 46.2 | 32.5 ± 0.8 | **1.14x** | 253.6 KiB |
| **Algorithms suite** | `<algorithm>`, `<numeric>` | 124.7 ± 0.5 | 176.8 | 28.9 ± 0.1 | **4.31x** | 3.23 MiB |
| **Containers suite** | `<vector>`, `<set>`, `<map>`, `<string>` | 240.7 ± 1.5 | 270.5 | 78.9 ± 0.9 | **3.05x** | 5.24 MiB |
| **I/O streams suite** | `<iostream>`, `<sstream>` | 237.7 ± 3.4 | 289.5 | 54.4 ± 0.4 | **4.37x** | 5.53 MiB |
| **Full CP bundle** | `<iostream>`, `<vector>`, `<algorithm>`, `<set>`, `<map>`, `<queue>`, `<cmath>` | 403.7 ± 1.1 | 464.2 | 115.5 ± 0.5 | **3.50x** | 9.36 MiB |

### Key Observations

1. **Header parsing accounts for over 85–90% of cold parse latency:**
   The minimal baseline requires only ~37 ms. Including standard containers or I/O streams expands parse time to ~240 ms (~200 ms of header parsing). Including the full competitive programming bundle pushes latency to ~404 ms (~367 ms of pure header parsing).
2. **Precompiled preambles yield a 3.5x–4.4x speedup on typical solutions:**
   With a precompiled preamble, warm parsing of the full CP bundle drops from 403.7 ms to 115.5 ms (saving ~288 ms per run). For algorithmic and stream suites, parse time drops below 60 ms.
3. **PCH disk footprint is moderate:**
   A full CP bundle generates a 9.36 MiB PCH file on disk. A user's cache containing 2–3 standard configurations consumes under 30 MiB of disk storage.
4. **Compilation overhead is easily amortized:**
   Generating the PCH takes ~460 ms once. On subsequent edits or on-save analyses, the cache saves hundreds of milliseconds on every invocation.

## Decision

We adopt a secure, isolated precompiled preamble caching architecture for BigO Lens:

### 1. Storage Location and Permissions

- **Path:** Cached preambles are stored strictly in the user-owned cache directory:
  - Linux/POSIX: `$XDG_CACHE_HOME/bigolens/preambles/` (defaulting to `~/.cache/bigolens/preambles/`).
  - Windows: `%LOCALAPPDATA%\bigolens\preambles\`.
  - macOS: `~/Library/Caches/bigolens/preambles/`.
- **Permissions:** The directory and all cached PCH files are created with strict permissions `0700` (`rwx------` for directories, `rw-------` for files).
- **Prohibitions:** Preambles must **never** be written to `/tmp`, system-wide directories, or workspace directories where other users or untrusted processes could inspect or tamper with them.

### 2. Cryptographic Cache Keying

A preamble cache entry is keyed by a SHA-256 digest over the exact compilation tuple:
- **Compiler identity:** Hash of the Clang binary path, binary size, and `clang --version` identifier;
- **Standard flags:** Language dialect (`-std=c++20`), target triple, and system include directories;
- **Preamble content:** Exact text of the leading `#include` directives up to the first non-preprocessor token.

The on-disk artifact is named `<sha256_hash>.pch`.

### 3. Fail-Closed Fallback and Invalidation

- If a cached PCH file is missing, corrupted, unreadable, or was compiled with mismatched compiler flags, the engine falls back silently and immediately to a standard cold parse.
- Hash collisions or malformed preamble headers never cause an engine crash or panic.
- Cache corruption is handled by unlinking the corrupted entry and falling back to cold parsing.

### 4. Workspace Trust Boundary

Under [THREAT_MODEL.md §4](../../engineering/THREAT_MODEL.md) and VS Code Workspace Trust (BL-506):
- In **untrusted workspaces**, preamble caching is restricted or disabled.
- Only trusted standard headers (`<iostream>`, `<vector>`, etc.) from system directories may be cached in preambles. Custom user headers (`#include "my_header.hpp"`) are **never** cached across workspace boundaries.

### 5. Determinism Invariant

> **Preamble Invariant:** An analysis response generated with a precompiled preamble cache hit MUST be bit-for-bit identical to the response generated from a cold AST parse without preamble caching.

No complexity result, bound status, confidence score, finding list, or diagnostic may differ based on cache hit status.

## Alternatives Considered

1. **Long-lived persistent engine daemon:**
   Keeping a background Clang process alive in memory.
   *Rejected for initial implementation:* Daemons create complex lifecycle issues (crash recovery, memory leaks, zombie process tracking, IPC synchronization across editor window reloads). Preamble caching achieves 3.5x–4.5x speedups while maintaining the stateless, robust process-per-request architecture ([ADR 0004](0004-json-ipc.md)).
2. **Bundling precompiled headers in the VSIX extension package:**
   Shipping static `.pch` files inside the release package.
   *Rejected:* Clang PCH files are tightly coupled to the exact compiler version, LLVM build configuration, and host standard library headers (e.g. host `libstdc++` version). A bundled PCH fails to load across different Linux distributions or compiler updates. Generating preambles on-demand on the host machine guarantees compatibility.
3. **Regular expression or heuristic text filtering:**
   Skipping headers via text substitution or regex parsing.
   *Rejected:* Prohibited by [RULES.md §17.1](../../../RULES.md#17-prohibited-practices). Analysis must operate on the complete, valid Clang AST.

## Consequences

- **Positive:**
  - Reduces cold parse latency from >400 ms down to ~115 ms for typical competitive programming files.
  - Enables smooth on-save analysis (BL-505) and responsive tree updates without noticeable UI lag.
  - Maintains strict determinism and memory safety boundaries.
- **Negative:**
  - One-time cache compilation latency (~460 ms) on the first analysis of a new header combination.
  - Consumes ~10–30 MiB of disk space in the user's cache directory.
  - Requires cache eviction/pruning policies (LRU or TTL) to prevent unbounded growth over time.
