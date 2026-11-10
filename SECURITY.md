# Security Policy

## Supported versions

BigO Lens is pre-1.0. Security fixes are made **only on the latest `main`** and included in the next release. Older `0.x` releases are not patched.

| Version | Supported |
| --- | --- |
| `main` (latest) | Yes |
| Any `0.x` release other than the latest | No. Upgrade to the latest |

## Reporting a vulnerability

**Please do not open a public issue, discussion or pull request for a security problem.**

Report it privately through **GitHub private vulnerability reporting**:

1. Go to the repository's **Security** tab (`https://github.com/chowdhuryrifat/bigo-lens/security`).
2. Click **Report a vulnerability**.
3. Describe the issue, the affected component (engine, extension, build or CI), the version or commit, reproduction steps, and the impact you expect.

There is no separate security email address. Private vulnerability reporting is the only channel.

What to expect:

- The maintainers acknowledge the report through the advisory, and keep you updated there.
- We work with you on a fix and a coordinated disclosure. When the fix is released, we publish a GitHub Security Advisory crediting you, unless you prefer not to be named.
- This is a volunteer project, so we cannot promise fixed response times. We treat reports with priority.

## Scope

In scope:

- The native engine (`bigolens-engine`): request parsing and validation, Clang front-end usage, resource limits
- The VS Code extension: engine discovery and spawning, constraints-file handling, response handling, and any future webview
- Build and release: CMake dependency fetching, GitHub Actions workflows, release artifacts
- Anything that lets untrusted input (source code, constraints files, requests or responses) cause code execution, file access beyond what is documented, or denial of service beyond the documented limits

Out of scope:

- Vulnerabilities in Clang/LLVM, VS Code, Node.js or other upstream projects. Report those upstream, but tell us if BigO Lens makes them exploitable.
- The documented behaviour that Clang reads headers named in `#include` directives (see [Current security posture](#current-security-posture))
- Results that are incorrect but not security-relevant. Report those as bugs.
- Issues that require an attacker who can already modify your user settings, environment variables or installed binaries

## Current security posture

This section describes the code **as it is today**:

- **The engine never executes user code.** It parses source with Clang in `-fsyntax-only` mode, from an in-memory buffer, with a fixed compiler argument list. The only request-derived compiler input is the validated `cpp_standard` value. Nothing is compiled to a binary, linked or run.
- **Like any compiler, Clang reads the headers named in `#include` directives** from your file system. The engine returns findings and diagnostics only, never file contents.
- **Input limits:** requests are limited to 4 MiB, source to 2 MiB, and JSON nesting depth to 64 (checked before parsing). Constraint values are limited to magnitude 10^18, and expression evaluation has a node budget. Unknown request fields are rejected.
- **Failures are structured.** The request handler converts errors into JSON error responses, and stdout carries only the single JSON response.
- **The extension** spawns the engine without a shell (`shell: false`), with no arguments, a timeout and a 16 MiB output cap. It validates every engine response and every constraints file against JSON Schemas.
- **Benchmarking is not implemented.** The `benchmark` action returns `not_implemented`. When benchmarking is built (milestone M4), it will compile and run user code. It **must not be described as sandboxed** until real platform isolation is implemented and tested. A timeout alone is not a sandbox. The requirements are in [THREAT_MODEL.md §4.2](docs/engineering/THREAT_MODEL.md#42-malicious-benchmark-programs-tb6-planned-m4).
- **There is no webview yet.** A future webview must use a strict Content Security Policy and nonce scripts.
- **Known gaps:** release artifacts are not signed. Workspace Trust is not yet declared. `bigoLens.enginePath` is a machine-scoped setting, so a workspace cannot override which engine executable runs.

The full analysis of assets, trust boundaries, threats and mitigations (implemented and planned) is in [docs/engineering/THREAT_MODEL.md](docs/engineering/THREAT_MODEL.md).

## Supply chain

- C++ dependencies (nlohmann/json 3.11.3, GoogleTest 1.15.2) are taken from the system when available. Otherwise, pinned release archives are downloaded and verified by **SHA-256**.
- npm dependencies are locked (`package-lock.json`) and installed with `npm ci`.
- CI runs CodeQL (C/C++ and JavaScript/TypeScript), dependency review on pull requests, and `npm audit`. Dependabot proposes updates weekly for GitHub Actions and npm.
- Workflows use official actions only, and run with `permissions: contents: read` unless a job needs more.

## Secret handling rules

- **Never commit secrets:** no tokens, API keys, passwords, private keys, `.env` files or credentials of any kind, in code, fixtures, examples, documentation or commit history.
- CI currently needs no secrets beyond the automatically scoped `GITHUB_TOKEN`. Any future secret (for example a Marketplace publishing token) must be stored as a GitHub encrypted secret, preferably in a protected environment, available only to the job that needs it, and never printed to logs.
- `.npmrc` files in this repository contain only project settings, never registry credentials.
- If a secret is committed by mistake, **revoke it immediately**. Removing it from history is not enough. Then tell the maintainers through a private vulnerability report.
- Do not put secrets, personal data or proprietary source code in bug reports. Reduce reproductions to a minimal example.
