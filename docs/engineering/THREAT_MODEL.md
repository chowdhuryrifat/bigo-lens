# Threat Model

This document identifies what BigO Lens must protect, where trust changes hands, and how each threat is mitigated. Every mitigation is labelled **Implemented**, **Partial** or **Planned (Mx)**. Planned mitigations are requirements for future work, not current guarantees.

Reporting process: [SECURITY.md](../../SECURITY.md). Benchmark design: [BENCHMARK_SPECIFICATION.md](../specifications/BENCHMARK_SPECIFICATION.md).

## 1. System summary and security posture today

- The **VS Code extension** (TypeScript) reads the active C++ file and an optional sidecar constraints file (`<file>.bigolens.json`). It spawns the **engine** and exchanges one JSON request and one JSON response over stdin and stdout.
- The **engine** (`bigolens-engine`, C++20) validates the request and parses the source with Clang LibTooling in `-fsyntax-only` mode, from an in-memory buffer. It then runs static analysis and returns findings.
- **The engine never compiles to object code, links or executes user code.** The `benchmark` action returns `not_implemented`. The benchmark library that exists today (`bigolens_benchmark`) only does statistics and curve fitting on numbers passed to it.
- Like any compiler front end, Clang **reads the headers named in `#include` directives** from the user's file system, using its normal search paths.

Most "code execution" threats therefore apply only to the planned M4 benchmark runner, and are recorded here so that M4 is designed against them.

## 2. Assets

| ID | Asset | Why it matters |
| --- | --- | --- |
| A1 | The user's machine: files, credentials, network identity | A local developer tool runs with the user's full privileges |
| A2 | The user's source code and constraints | May be private (contest solutions, proprietary code) |
| A3 | Integrity of analysis results | Users make decisions (submit, rewrite) based on them, and wrong certainty is harmful |
| A4 | Availability of the editor | A hung or runaway engine degrades VS Code |
| A5 | The project's supply chain: repository, CI, release artifacts | A compromised release would run on every user's machine |
| A6 | CI secrets and tokens | Could be used to tamper with releases |

## 3. Trust boundaries

```mermaid
flowchart LR
  subgraph USER["User workspace (untrusted content)"]
    SRC["C++ source file"]
    SIDE["Sidecar constraints file (*.bigolens.json)"]
    HDR["Headers reachable via include directives"]
  end

  subgraph VSC["VS Code extension host (trusted code, user privileges)"]
    EXT["BigO Lens extension"]
    SET["Settings: bigoLens.enginePath, env BIGOLENS_ENGINE_PATH"]
    WV["Webview report"]
  end

  subgraph ENG["Engine process (trusted code, untrusted input)"]
    VAL["Request validation (size, depth, schema)"]
    CLANG["Clang -fsyntax-only (in-memory TU)"]
    AN["Static analysis + workload + feasibility"]
  end

  subgraph BENCH["Benchmark sandbox (planned M4)"]
    CC["Compiler (fixed argv)"]
    RUN["User program under limits"]
  end

  subgraph SUPPLY["Supply chain"]
    DEPS["nlohmann/json, GoogleTest, npm packages, GitHub Actions"]
    REL["GitHub Releases: engine binary + .vsix"]
  end

  SRC -->|"TB1: read by extension"| EXT
  SIDE -->|"TB1: ajv-validated"| EXT
  SET -->|"TB2: selects executable"| EXT
  EXT -->|"TB3: spawn (shell: false), JSON on stdin"| VAL
  VAL --> CLANG
  HDR -->|"TB4: read by Clang"| CLANG
  CLANG --> AN
  AN -->|"TB3: one JSON line on stdout, schema-validated"| EXT
  EXT -->|"TB5: postMessage, schema-validated"| WV
  AN -.->|"TB6: user-initiated (planned)"| CC
  CC -.-> RUN
  DEPS -->|"TB7: pinned + hashed"| ENG
  DEPS -->|"TB7: lockfile + npm ci"| VSC
  REL -->|"TB8: downloaded by user"| VSC
```

| Boundary | From → To | Crossing data |
| --- | --- | --- |
| TB1 | Workspace → extension | Source text, sidecar JSON |
| TB2 | Settings and environment → extension | Engine executable path |
| TB3 | Extension ↔ engine process | JSON request on stdin, JSON response on stdout, stderr text |
| TB4 | File system → Clang | Included headers |
| TB5 | Extension ↔ webview | Rendered results, user actions |
| TB6 | Engine → compiler and user program (planned M4) | Source, argv, generated inputs, measurements |
| TB7 | Upstream dependencies → build | Source archives, npm packages, CI actions |
| TB8 | Release artifacts → users | Engine binary, `.vsix` |

## 4. Threats and mitigations

STRIDE categories: **S**poofing, **T**ampering, **R**epudiation, **I**nformation disclosure, **D**enial of service, **E**levation of privilege.

### 4.1 Untrusted source code (TB1, TB3, TB4)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Source crafted to crash the parser or analyzer | D | Clang is a hardened, widely fuzzed front end. Analysis never runs on an AST with errors. `RequestHandler` converts exceptions into `internal_error` | Implemented |
| Pathological source causing huge ASTs or deep recursion | D | Source limited to 2 MiB and the request to 4 MiB. The extension enforces a timeout (`bigoLens.analysisTimeoutMs`, default 30 s) and kills the engine | Implemented |
| Source that floods diagnostics | D | At most 50 error diagnostics are collected, and warnings are suppressed | Implemented |
| `#include` of sensitive files (for example `#include "/etc/shadow"`) to leak contents | I | The engine reads headers as a compiler would, but returns only diagnostics and findings, never file contents. Diagnostic messages could quote a fragment of the included text in an error. The output goes only to the local user who already has that access | Accepted risk (local, same user) |
| `#include` of a device or FIFO (`/dev/zero`, `/dev/stdin`) to hang the parser | D | The extension timeout kills the engine. Restricting include paths to regular files is not implemented | Partial |
| Source triggering exponential template instantiation | D | Extension timeout. Clang's own instantiation depth limits apply | Partial |
| Analysis results that falsely claim certainty | T (of A3) | Bound status and confidence rules. Unknown shapes are reported as `unknown` or `unsupported`. Fixture tests cover the near misses | Implemented (partial analyzer coverage) |

### 4.2 Malicious benchmark programs (TB6)

Dynamic benchmarking compiles and **executes** user code in the Rust engine (`crates/bigolens-engine`). That code may be hostile, for example a solution copied from an untrusted source, or a workspace opened from a malicious repository.

| Threat | STRIDE | Required mitigation | Status |
| --- | --- | --- | --- |
| Program reads or writes user files, or exfiltrates over the network | I, T, E | Platform isolation. Linux: unprivileged user, mount (`MS_RDONLY` root, masked `/home`, `/root`, and `/etc/passwd`, private `/tmp` tmpfs with writable `/tmp/scratch` tmpfs and read-only `/tmp`), network and PID namespaces (with benchmark running as PID 1 and fresh private `/proc` hidepid mount), full capability dropping (`PR_CAPBSET_DROP`, zeroed `capset`, securebits), plus seccomp-BPF filter with architecture checks (`AUDIT_ARCH`), x32 blocking, privilege escalation syscall blocking, and `execveat` argument restriction on the binary descriptor only. Windows and macOS: no sandbox container is available; the engine fails closed (`isolation_unavailable`) unless `allow_unsandboxed: true` is passed, and reports `isolation_level: "process-limits-only"` with `isolated: false` | Implemented (Linux, BL-405) |
| Fork bomb or runaway child processes | D | `CLONE_NEWPID` on Linux terminates all namespace descendants on child exit (benchmark runs as PID 1 via intermediate launcher); process-group `kill(-pgid)` and best-effort `RLIMIT_NPROC` | Implemented (Linux, BL-404) |
| Infinite loop | D | Wall-clock and CPU timeouts, then forced termination | Implemented (BL-404) |
| Memory exhaustion | D | Virtual address space (`RLIMIT_AS`) or Windows Job memory limits, with the measured peak reported | Implemented (BL-404) |
| Disk filling | D | `RLIMIT_FSIZE` and per-run temporary in-memory tmpfs scratch directory | Implemented (Linux, BL-404) |
| Benchmarking started without the user's knowledge | E | Benchmarks are user-initiated only, with an explicit command and confirmation. They never run on file open or save | Designed (ADR 0014, BL-401) |
| Users believing a timeout or Job Object is a sandbox | I | **A timeout or Job Object alone is not a sandbox.** On platforms without full isolation (Windows, macOS), the runner labels execution as `isolation_level: "process-limits-only"` with `isolated: false` and emits explicit warnings | Implemented (honest labelling, BL-404, BL-405) |

#### Linux Filesystem Isolation & Residual Readable Paths

The benchmark isolation environment on Linux implements multi-layered filesystem controls:
1. **Private Mount Namespace:** Created with `MS_REC | MS_PRIVATE` so mount events do not leak to or from the host.
2. **Read-Only Root:** The entire root filesystem is bind-mounted and remounted read-only (`MS_BIND | MS_REMOUNT | MS_RDONLY | MS_REC`).
3. **Sensitive Path Masking:**
   - `/home` is masked with an empty, non-executable tmpfs (`MS_NOSUID | MS_NODEV | libc::MS_NOEXEC`).
   - `/root` is masked with an empty, non-executable tmpfs (`MS_NOSUID | MS_NODEV | libc::MS_NOEXEC`).
   - `/etc/passwd` is masked by bind-mounting `/dev/null` over it.
   - Any missing paths (`ENOENT`) are skipped as "nothing to hide", while unexpected mount failures abort isolation.
4. **Private Temporary Filesystem:** `/tmp` is masked with a private empty tmpfs and remounted read-only, preventing the benchmark from viewing host temporary files or writing directly to `/tmp`. A writable tmpfs is mounted at `/tmp/scratch`, and the benchmark process `chdir`s into `/tmp/scratch`. Outside `/tmp/scratch`, writing fails with `EROFS`.
5. **Residual Readable Filesystem:** System directories (`/usr`, `/lib`, `/bin`, and `/etc` minus `/etc/passwd`) remain readable to allow dynamic linking (`ld.so`), loading C/C++ runtime shared libraries (`libc.so`, `libstdc++.so`), and reading basic locale/time zone data. Code executing inside the benchmark cannot modify these files or persist changes past process termination.

### 4.3 Compiler argument injection (TB3, TB6)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| A request field smuggles compiler flags (`-Xclang -load`, `-fplugin=`, `-include /etc/passwd`, `-o /path`) | E, I | The Clang front end uses a **fixed argument list**. The only request-derived value is `options.cpp_standard`, validated against the enum `c++11 … c++23` before use. Unknown request fields are rejected | Implemented |
| A future benchmark compiler invocation takes flags from the user | E | The compiler is invoked with a fixed argv. Optimization level and standard come from validated enums, and free-form flags are never accepted | Implemented (BL-402) |
| A source path used as an option (a file named `-fplugin=x.so`) | E | Source is passed as an in-memory buffer under a display name, written to a fixed `solution.cpp` inside a private temp dir. Operands in Clang are preceded by `--`; GCC operands are sanitized with `./` | Implemented (BL-402) |
| Compilation hangs or exhausts memory via template recursion or large constexpr | D | Compiler invocation has a 15-second wall-clock timeout and 512 KiB output limit with process-group termination (`kill(-pgid)`), plus resource limit flags (`-ftemplate-depth=256`, `-fconstexpr-depth=256`, `-fconstexpr-steps=1048576`/`-fconstexpr-ops-limit=10000000`, `-fmax-errors=10`) | Implemented |
| Sensitive file path leakage through compiler diagnostics (e.g. `#include "/etc/shadow"`) | I | Absolute host paths outside the compile scratch directory are redacted to `[redacted_path]` in compiler diagnostics, messages, and raw stderr before returning to the user | Implemented |

### 4.4 Shell injection and process spawning (TB2, TB3)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Shell metacharacters in the engine path or file names executed by a shell | E | The extension spawns the engine with `child_process.spawn(path, [], { shell: false })`. No arguments are passed, and the request travels over stdin | Implemented |
| A malicious workspace sets `bigoLens.enginePath` to an attacker binary | E, S | The setting has scope `machine`, so VS Code ignores it in workspace and folder settings; only user (machine) settings and the `BIGOLENS_ENGINE_PATH` environment variable can select a different executable. In addition, Workspace Trust support (`capabilities.untrustedWorkspaces`) is declared and enforced in the engine locator to ignore workspace-level configuration in untrusted workspaces | Implemented (BL-506) |
| `BIGOLENS_ENGINE_PATH` or the development-build fallback picks up an unexpected binary | S | The search order is documented. The engine identity is checked via `engine.name` and `protocol_version` in every response. There is no signature verification | Partial |
| The engine uses `system()` or `popen()` | E | Prohibited by [RULES.md §16](../../RULES.md#16-prohibited-practices). The engine spawns no processes today | Implemented (no spawning) |

### 4.5 Path traversal (TB1, TB6)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| `source.path` in a request used to read or write arbitrary files | T, I | The engine treats `source.path` as a **display name only**. The content comes from `source.content`, and the engine writes no files | Implemented |
| The sidecar file location is manipulated | T | The sidecar path is derived from the active document's path (`<file>.bigolens.json`), not from file content | Implemented |
| Benchmark artifacts written outside their temporary directory | T | Fixed source file name `solution.cpp`, unique temporary directories via `tempfile` (mode 0700), never user-controlled filesystem paths | Implemented |

### 4.6 Temporary directories (TB6)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Predictable temporary names enabling symlink or race attacks | T, E | Use `mkdtemp` or the platform equivalent (mode 0700) per run. Never reuse a directory | Planned (M4). No temporary files are created today |
| Leftover binaries or inputs after a crash | I, D | RAII cleanup on every path, plus cleanup of stale directories carrying a BigO Lens prefix on start-up | Planned (M4) |

### 4.7 Resource exhaustion (TB3)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Huge request | D | Limited to 4 MiB (`request_too_large`), with reads bounded in `main.cpp` | Implemented |
| Deeply nested JSON overflowing the recursive parser | D | Depth limited to 64, **checked before parsing** (reported as `malformed_json`) | Implemented |
| Huge constraint values causing overflow | D, T | Magnitude at most 10^18. The evaluator reports non-finite values as `Overflow`, and memory sums saturate | Implemented |
| Non-affine summations with enormous ranges | D | Evaluator budget of 5×10^7 node evaluations by default, then Unknown or approximate | Implemented |
| Engine writes unbounded stdout | D | The extension caps stdout at 16 MiB and kills the process beyond it | Implemented |
| Engine hangs | D | Extension timeout and kill | Implemented |

### 4.8 Malformed JSON (TB1, TB3)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Invalid request JSON | D | `malformed_json` structured error, no crash | Implemented |
| Schema-valid but semantically hostile values | T | Strict validation: unknown fields rejected, `request_id` pattern `[A-Za-z0-9_.:-]{1,128}`, enums checked, ranges checked, identifiers at most 64 characters | Implemented |
| Malformed or hostile engine response (a buggy or replaced engine) | T, E | The extension validates every response with ajv against `response.schema.json` before use | Implemented |
| Malformed sidecar constraints | T | Validated with ajv against the `constraints` sub-schema, with errors reported to the user | Implemented |

### 4.9 Dependency compromise (TB7, TB8)

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Tampered C++ dependency archive | T | FetchContent downloads pinned release URLs verified by **SHA-256**. A system `find_package` is preferred when available | Implemented |
| Malicious npm package or update | T, E | `package-lock.json` with `npm ci`, `npm audit` in `security.yml`, Dependabot (weekly), and dependency review on pull requests | Implemented |
| Compromised GitHub Action | T, E | Only official actions, pinned to major versions, with `permissions: contents: read` by default. Pinning to commit SHAs is a possible hardening step | Partial |
| Static-analysis gaps in our own code | T | CodeQL for C/C++ and JavaScript/TypeScript | Implemented |
| Tampered release artifacts | T, S | Artifacts are built by `release.yml` from the tagged commit. Signing, checksums and provenance attestations are not implemented | Planned |
| Leaked CI secrets | I | No secrets are required for CI today. Release jobs use the scoped `GITHUB_TOKEN`. Secrets must never be printed | Implemented |

### 4.10 Webview security (TB5)

The M5 report webview is implemented with strict defense-in-depth controls:

| Threat | STRIDE | Mitigation | Status |
| --- | --- | --- | --- |
| Script injection (XSS) via source identifiers or analysis strings | T, E | Strict Content Security Policy: `default-src 'none'`, scripts only with a fresh per-load cryptographically random **nonce** (`script-src 'nonce-${nonce}'`), styles only from extension resources (`style-src ${webview.cspSource} 'nonce-${nonce}'`), no `unsafe-eval`, no `unsafe-inline`. All dynamic text (function names, messages, expressions) strictly escaped via `escapeHtml` and inserted as text | Implemented |
| Access to arbitrary workspace or local files | I | `localResourceRoots` strictly limited to `[vscode.Uri.joinPath(extensionUri, 'media')]` | Implemented |
| Hostile or malformed messages from webview to extension | T, E | Every `postMessage` received from the webview is strictly validated against `validateWebviewMessage` rejecting unknown types or malformed payloads | Implemented |
| Remote data exfiltration or CDN compromise | I | No remote origins allowed in CSP (`default-src 'none'`, offline bundled assets only, no remote CDNs or fonts) | Implemented |

### 4.11 External input validation: summary

| Input | Validated by | Rules |
| --- | --- | --- |
| Engine request | `api::parseRequest` (C++) | Size, depth, schema version, closed field sets, types, ranges, identifier syntax |
| Engine response | ajv in the extension | `response.schema.json` |
| Sidecar constraints file | ajv in the extension, then the engine again | `constraints` schema, re-validated by the engine |
| Webview messages (`postMessage`) | `validateWebviewMessage` in the extension | Object with recognized `type`, valid integer coordinates, rejected otherwise |
| VS Code settings | VS Code schema in `package.json` | Enum for `cppStandard`, range 1000–600000 for `analysisTimeoutMs` |
| Environment (`BIGOLENS_ENGINE_PATH`, `BIGOLENS_CLANG_RESOURCE_DIR`) | Existence checks only | Treated as trusted user configuration |
| Benchmark inputs and sizes (M4) | To be specified | Bounded sizes, generated by the engine, never from free-form shell commands |

## 5. Repudiation and logging

BigO Lens is a local tool and does not keep audit logs. Requests carry a `request_id` that is echoed in the response, and the extension logs request IDs and error codes to the `BigO Lens` output channel. Source content is never logged.

## 6. Residual risks

1. Clang reads any header the source includes. A user who analyses untrusted code on a machine with sensitive readable files accepts the same exposure as when compiling that code.
2. Engine-binary resolution trusts the user's settings and environment, including workspace-level settings (see §4.4).
3. Dynamic benchmarking is isolated on Linux via namespaces, seccomp-BPF, capability dropping, and path masking. However, host system directories (`/usr`, `/lib`, `/bin`, `/etc`) remain readable so dynamically linked binaries can execute. On Windows and macOS, isolation is unavailable; programs run under process limits only with an explicit modal warning.
4. Benchmark compilation runs the host C++ compiler in the scratch directory with a cleared environment, but without filesystem namespace isolation (so system headers remain accessible). While paths outside the scratch directory in diagnostics are redacted to `[redacted_path]`, compiler error messages quoting lines from included files could still expose local file fragments to the user if sensitive files are included via `#include "/abs/path"`.
5. Release artifacts are not signed.

## 7. Review triggers

Update this document in the same pull request when a change:

- spawns a process or adds a new process boundary
- reads or writes files, or creates temporary directories
- adds a request field that influences compiler or process arguments
- adds a webview, network access or a new dependency
- changes CI permissions or release packaging
