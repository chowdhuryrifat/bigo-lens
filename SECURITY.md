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

- **Static analysis never executes user code.** The `analyze` action parses source with Clang in `-fsyntax-only` mode (or tree-sitter / regex AST in the multi-language parser), from an in-memory buffer, with a fixed compiler argument list. The only request-derived compiler input is the validated `cpp_standard` value.
- **Like any compiler, Clang reads the headers named in `#include` directives** from your file system. The engine returns findings and diagnostics only, never file contents.
- **Input limits:** requests are limited to 4 MiB, source to 2 MiB, and JSON nesting depth to 64 (checked before parsing). Constraint values are limited to magnitude 10^18, and expression evaluation has a node budget. Unknown request fields are rejected.
- **Failures are structured.** The request handler converts errors into JSON error responses, and stdout carries only the single JSON response.
- **The extension** spawns the engine without a shell (`shell: false`), with no arguments, a timeout and a 16 MiB output cap. It validates every engine response and every constraints file against JSON Schemas.
- **Benchmarking isolation (Rust engine):** The `benchmark` action compiles and executes user code with strictly enforced platform isolation where available. On Linux, unprivileged namespaces (`CLONE_NEWUSER`, `CLONE_NEWNET`, `CLONE_NEWNS`, `CLONE_NEWPID`), a read-only root mount (`MS_RDONLY`), masked `/home`, `/root`, and `/etc/passwd`, a private read-only `/tmp` with an in-memory writable `tmpfs` scratch directory at `/tmp/scratch`, full capability dropping, and a hardened seccomp-BPF syscall deny-list with argument-filtered `execveat` (permitting only a single `execveat` of the target binary and blocking network, tracing, and privilege escalation syscalls) are enforced. The seccomp filter uses a hardened deny-list rather than a default-deny allow-list to maintain compatibility across varied standard library and runtime implementations while relying on namespace and capability controls for default-deny isolation. On platforms without strong sandbox isolation (Windows and macOS), execution is explicitly reported as `isolation_level: "process-limits-only"` with `isolated: false` and user warnings, and the engine fails closed with `isolation_unavailable` unless `allow_unsandboxed: true` is explicitly opted in.
- **Webviews use strict CSP and nonce scripts.** The constraint configuration form and report panels enforce a restrictive Content Security Policy and per-session cryptographically random nonces.
- **Workspace Trust is enforced.** In untrusted workspaces, BigO Lens operates in restricted mode, ignoring workspace-scoped `bigoLens.enginePath` overrides and refusing benchmark execution.
- **Release artifacts are attested and checksummed.** All release artifacts include SHA-256 checksums (`SHA256SUMS.txt`) and signed SLSA build provenance attestations.

The full analysis of assets, trust boundaries, threats and mitigations (implemented and planned) is in [docs/engineering/THREAT_MODEL.md](docs/engineering/THREAT_MODEL.md).

## Release artifact verification

Official releases on GitHub Releases include:
1. Platform-specific `.vsix` packages (`linux-x64`, `win32-x64`, `darwin-arm64`, `darwin-x64`) and standalone engine archive bundles (`.tar.gz` and `.zip`).
2. A cryptographic checksum manifest: `SHA256SUMS.txt`.
3. Signed SLSA build provenance attestations generated via GitHub's Sigstore OIDC authority (`actions/attest-build-provenance`).

Users, security teams, and automated environments should verify artifact integrity and authenticity prior to installation.

### 1. Verifying SHA-256 Checksums

Download the release artifact and `SHA256SUMS.txt` from the GitHub release page, then verify:

- **Linux:**
  ```bash
  sha256sum --check --ignore-missing SHA256SUMS.txt
  ```
- **macOS:**
  ```bash
  shasum -a 256 --check --ignore-missing SHA256SUMS.txt
  ```
- **Windows (PowerShell):**
  ```powershell
  Get-FileHash -Algorithm SHA256 .\bigo-lens-*.vsix
  # Compare the resulting Hash string with the corresponding entry in SHA256SUMS.txt
  ```

### 2. Verifying Cryptographic Build Provenance

We use GitHub Artifact Attestations (`actions/attest-build-provenance`) backed by Sigstore to cryptographically prove that each release artifact was built on official GitHub-hosted runners directly from the release commit in `chowdhuryrifat/bigo-lens`.

Verify the attestation using the [GitHub CLI (`gh`)](https://cli.github.com/):

```bash
# Verify a specific platform package:
gh attestation verify bigo-lens-0.1.0-linux-x64.vsix --repo chowdhuryrifat/bigo-lens

# Or verify all downloaded artifacts in the directory:
gh attestation verify * --owner chowdhuryrifat
```

A successful verification confirms that:
- The artifact was built on official GitHub Actions runners.
- The build was initiated by the official release workflow (`.github/workflows/release.yml`).
- The source commit matches the exact tagged release in the repository.
- The artifact has not been modified or tampered with in transit.

## Supply chain

- C++ dependencies (nlohmann/json 3.11.3, GoogleTest 1.15.2) are taken from the system when available. Otherwise, pinned release archives are downloaded and verified by **SHA-256**.
- npm dependencies are locked (`package-lock.json`) and installed with `npm ci`.
- CI runs CodeQL (C/C++ and JavaScript/TypeScript), dependency review on pull requests, and `npm audit`. Dependabot proposes updates weekly for GitHub Actions and npm.
- Workflows use official actions only, and run with `permissions: contents: read` unless a job needs more.
- Release workflows generate cryptographic build provenance attestations for all release artifacts and require manual reviewer approval via the `marketplace` environment before publishing.

## Secret handling rules

- **Never commit secrets:** no tokens, API keys, passwords, private keys, `.env` files or credentials of any kind, in code, fixtures, examples, documentation or commit history.
- Publication credentials (`VSCE_PAT`, `OVSX_PAT`) are stored as encrypted secrets scoped exclusively to the protected GitHub Actions Environment named `marketplace`. Access to this environment requires **mandatory human reviewer approval**, ensuring no automated push can publish artifacts without review.
- `.npmrc` files in this repository contain only project settings, never registry credentials.
- If a secret is committed by mistake, **revoke it immediately**. Removing it from history is not enough. Then tell the maintainers through a private vulnerability report.
- Do not put secrets, personal data or proprietary source code in bug reports. Reduce reproductions to a minimal example.
