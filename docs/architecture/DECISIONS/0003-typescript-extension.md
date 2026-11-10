# ADR 0003: Build the editor integration as a TypeScript VS Code extension

- **Status:** Accepted
- **Date:** 2026-11-10

## Context

The target users write C++ in an editor, often for competitive programming, and want complexity feedback next to their code: a results view, Problems-panel entries, and constraint editing per file. The integration needs a mature extension API, testability in CI, and a path to a richer UI later (webview reports and charts, Planned M5).

## Decision

Implement the client as a **TypeScript VS Code extension** (Node 20, VS Code `^1.85`) in `extension/`:

- `src/commands/`: `bigoLens.analyzeActiveFile`, `bigoLens.editConstraints`, `bigoLens.showCapabilities`.
- `src/services/`: engine locator, constraints sidecar (`<source file>.bigolens.json`), request builder, engine client, and response validation with ajv against schemas copied from `shared/schemas/`.
- `src/providers/`: the "Analysis Results" tree view in the "BigO Lens" activity-bar container, Information-level diagnostics, and the "BigO Lens" output channel.
- `src/models/`: protocol types.

The extension contains **no analysis logic**. It is a thin client of the engine.

Tooling: `tsc`, ESLint (typescript-eslint flat config), Prettier, node:test unit tests, an integration test against the real engine (skipped with a message when no engine is available), and an extension-host smoke test with `@vscode/test-electron` under xvfb in CI.

## Alternatives considered

| Alternative | Why it was rejected |
|---|---|
| Language Server Protocol server in C++ | Larger surface. Persistent-process concerns arrive too early (see [ADR 0004](0004-json-ipc.md)). Not needed for on-demand analysis. |
| JetBrains or CLion plugin first | Smaller share of the target audience. Kotlin/JVM toolchain. |
| JavaScript without types | The protocol types and schema handling benefit from static typing. |
| CLI only | No inline feedback, which is the main purpose of the product. |

## Consequences

**Positive**

- Large user base and a stable API. The extension-host test harness runs in CI.
- Strict separation: UI changes never touch the engine, and the reverse is also true.
- Responses are schema-validated before they are displayed.

**Negative**

- Tied to VS Code and compatible editors.
- A Node toolchain and npm dependencies to maintain. Dependabot and `npm audit` mitigate the supply-chain risk.
- The future webview needs a strict CSP and nonce scripts ([THREAT_MODEL](../../engineering/THREAT_MODEL.md)).

## Follow-up considerations

- Webview report, charts and benchmark controls (M5).
- Marketplace publishing is out of scope for now. Releases attach a `.vsix` to GitHub Releases.
- If other editors are needed, an LSP adapter could reuse the same engine protocol.
