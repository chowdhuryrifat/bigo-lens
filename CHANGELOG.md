# Changelog

All notable changes to BigO Lens will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The JSON protocol is versioned separately (see [JSON_PROTOCOL.md](docs/specifications/JSON_PROTOCOL.md)).

## [Unreleased]

### Added

- Per-platform `.vsix` packages, release signing & provenance, and marketplace publication strategy (BL-603, BL-604, BL-605):
  - Added ADR 0013 (`docs/architecture/DECISIONS/0013-marketplace-and-open-vsx-publication.md`) establishing distribution strategy targeting Visual Studio Marketplace and Open VSX under publisher `bigolens`, token isolation via GitHub Actions Environment `marketplace` with mandatory manual reviewer approvals, and sovereign GitHub Releases fallback.
  - Multi-platform packaging in `.github/workflows/release.yml` targeting `linux-x64`, `win32-x64`, `darwin-arm64`, and `darwin-x64`, bundling platform-specific engine binaries and generating standalone engine archives (`.tar.gz` and `.zip`).
  - Automated SHA-256 checksum generation (`SHA256SUMS.txt`) and SLSA cryptographic build provenance attestations via `actions/attest-build-provenance` across all release artifacts.
  - Least-privilege marketplace and Open VSX publication job in `.github/workflows/release.yml` scoped to protected `marketplace` environment.
  - Verification guide in `SECURITY.md` documenting SHA-256 checksum and `gh attestation verify` commands.

- Header parsing cost evaluation and precompiled preamble cache architecture (BL-607):
  - Benchmarking and measurement tool `scripts/benchmark/measure-header-parse.py` measuring Clang syntax-only compilation and AST parse times across five representative C++ suites (minimal baseline, algorithms, containers, I/O streams, and full competitive programming bundle) under cold AST parsing versus Clang precompiled header (PCH) compilation.
  - Comprehensive unit test suite in `scripts/benchmark/test_measure_header_parse.py` verifying metrics calculation (median, MAD, CPU time, speedup factors), compiler command builders, markdown and JSON formatting, and mocked benchmark execution.
  - Architecture Decision Record `docs/architecture/DECISIONS/0014-preamble-cache-evaluation.md` evaluating empirical findings (demonstrating 3.5x–4.4x speedup on typical solutions, reducing parse latency from ~404 ms to ~115 ms) and specifying secure user-owned cache architecture in `$XDG_CACHE_HOME/bigolens/preambles/` with strict `0700` permissions, SHA-256 cache keying, fail-closed fallback to cold parse, workspace trust boundaries, and bit-for-bit response determinism.

- Validation corpus and automated scoring harness (BL-601):
  - Curated validation corpus in `tests/corpus/` containing 20 self-contained representative algorithmic solutions across linear loops, two-pointer loops, sliding windows, multi-level nested loops, grid traversals, triangular summations, logarithmic halving loops, divide-and-conquer recurrences (merge sort, binary search), STL algorithms and containers (`std::sort`, `std::set`), dynamic programming with nested 2D vectors, and unprovable near-miss patterns (Collatz loops, iterative binary search without monotonicity proofs, naive Fibonacci recursion, Euclidean GCD) verifying the Prime Directive.
  - Machine-readable manifest `tests/corpus/manifest.json` defining exact ground truth for time/space complexity, constraints, and classification rationales.
  - Automated scoring CLI `scripts/test/score-corpus.py` reporting accuracy, precision, unprovable rate, and regression detection against manifest with `--fail-on-regression`.
  - Comprehensive unit test suite `scripts/test/test_score_corpus.py` verifying metric calculation, threshold enforcement, report generation, and CLI exit codes.
  - Automated CI scoring step in `.github/workflows/ci.yml` and integration into `scripts/test/run-all-tests.sh`.

- Protocol stability and formal deprecation policy (BL-606):
  - Protocol stability review ahead of v1.0 documented in `docs/specifications/JSON_PROTOCOL.md §8.1`.
  - Defined semver guarantees, 6-month deprecation lifecycle (`active` -> `deprecated` -> `removed`), migration path requirements, schema validation test suite, and telemetry-free deprecation warnings.
