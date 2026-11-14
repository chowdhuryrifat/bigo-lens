# ADR 0013: VS Code Marketplace and Open VSX Publication Strategy

- **Status:** Accepted
- **Date:** 2026-11-14
- **Related issues:** BL-603, BL-604, BL-605

## Context

BigO Lens ships as a VS Code extension paired with platform-specific native analysis engine binaries. In [ADR 0011](0011-self-contained-engine-packaging.md), we established self-contained Linux `.vsix` packaging, bundling a statically linked engine with Clang builtin resource headers. [BL-603](../../development/IMPLEMENTATION_PLAN.md#bl-603-per-platform-vsix-packages) expands packaging to target all major developer platforms: Linux x64, Windows x64, macOS Apple Silicon (arm64), and macOS Intel (x64).

Distributing binary-bundled editor extensions introduces distinct distribution and operational challenges:
1. **Developer accessibility:** Developers expect one-click installations and automated background updates from their editor's native extension gallery.
2. **Ecosystem fragmentation:** The developer community is divided between the proprietary Microsoft Visual Studio Code Marketplace and the vendor-neutral, open-source Eclipse Open VSX registry (used by VSCodium, Eclipse Theia, Gitpod, Coder, and other alternative runtimes).
3. **Supply chain security and credential risk:** Automated publishing requires high-privilege access tokens (`VSCE_PAT`, `OVSX_PAT`). Storing credentials in automated continuous integration workflows creates serious supply chain risks—such as accidental leakage, compromised build steps, or unauthorized releases on repository events ([THREAT_MODEL.md §4.9](../../engineering/THREAT_MODEL.md#49-dependency-compromise-tb7-tb8)).
4. **Sovereignty and auditability:** Relying exclusively on third-party commercial registries subjects users to single-point-of-failure distribution risks, regional outages, or marketplace account suspensions.

## Decision

1. **Dual Registry Targeting Under Publisher `bigolens`**:
   - BigO Lens targets both the **Visual Studio Marketplace** and the **Open VSX Registry** under the verified, unified publisher identifier `bigolens`.
   - The publisher identifier in `extension/package.json` is set to `bigolens`.

2. **Per-Platform Distribution Packages (BL-603)**:
   - Rather than publishing a bloated universal package, release pipelines compile and package dedicated per-platform `.vsix` targets via `@vscode/vsce package --target <target>`:
     - `linux-x64`
     - `win32-x64`
     - `darwin-arm64`
     - `darwin-x64`
   - Each package bundles the exact native engine executable (`bigolens-engine` or `bigolens-engine.exe`) for that architecture, minimizing disk footprint and bandwidth consumption for users.

3. **Strict Credential Isolation via Protected GitHub Environment (BL-605)**:
   - Publication credentials (`VSCE_PAT` for VS Code Marketplace, `OVSX_PAT` for Open VSX) are **never** configured as general repository secrets.
   - Credentials are scoped exclusively to a dedicated GitHub Actions Environment named `marketplace`.
   - The `marketplace` environment requires **mandatory human reviewer approval** before any workflow job can access its secrets or execute publication commands.
   - The publication job operates under least-privilege permissions (`contents: read`) and executes only against verified release tags after human approval.

4. **GitHub Releases as Sovereign Fallback with Cryptographic Attestation (BL-604)**:
   - Standalone `.vsix` packages and engine archive bundles (`.tar.gz` / `.zip`) are published directly to GitHub Releases for every tagged release.
   - Users in air-gapped environments, enterprise networks, or without external marketplace access can install `.vsix` packages directly (`code --install-extension <file>.vsix`).
   - Every release artifact is accompanied by a SHA-256 checksum manifest (`SHA256SUMS.txt`) and signed SLSA build provenance attestations generated via GitHub's native `actions/attest-build-provenance`. Users and automated tools can verify authenticity using `gh attestation verify`.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| **GitHub Releases only (no marketplace publication)** | Impairs adoption and usability. Everyday developers expect extension discovery, ratings, and seamless automated updates from within VS Code and VSCodium. |
| **Fully automated publishing on every tag push without an approval gate** | High supply chain vulnerability. A compromised developer machine or malicious tag push could immediately publish unauthorized binaries to millions of user machines before review. |
| **Visual Studio Marketplace only** | Excludes the growing community of developers using VSCodium, Eclipse Theia, Code-server, and privacy-focused open source editor distributions. |
| **Fat/universal `.vsix` packaging** | Bundling native binaries for all 4 platforms in a single `.vsix` would increase the package download size fourfold (over 100 MB), wasting user bandwidth and storage. |

## Consequences

- **Minimal attack surface:** Publishing credentials are completely inaccessible to normal pull requests, commit pushes, or automated CI runs. They cannot be extracted by compromised branch builds.
- **Credential isolation:** Tokens have granular permissions restricted solely to publishing under the `bigolens` namespace.
- **Reproducible and auditable releases:** Cryptographic provenance attestations enable anyone to independently verify that a `.vsix` was built untampered from official repository source code on GitHub runners.
- **Resilience:** If either registry suffers downtime or credential revocation, the sovereign GitHub Releases channel remains fully functional.
