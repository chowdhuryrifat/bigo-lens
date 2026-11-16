# 0014 Benchmark Isolation Architecture

- **Status:** Accepted
- **Date:** 2026-11-12
- **Milestone:** M4 Dynamic benchmarking (BL-401)
- **Deciders:** BigO Lens Core Contributors

## Context

Milestone M4 introduces empirical benchmarking: compiling user-supplied code (or executing an interpreted script), feeding generated inputs parameterised by problem constraints, measuring wall-clock time and peak RSS, and fitting empirical growth models to derive runtime ranges.

Unlike static AST analysis, dynamic benchmarking compiles and **executes untrusted code**. That code may originate from competitive programming platforms, untrusted repositories, or malicious actors. Potential threats include:
1. **Hostile actions:** Arbitrary file reads (`$HOME`, `/etc/shadow`), file modifications, or network exfiltration.
2. **Denial of Service (DoS):** Fork bombs (`while(1) fork()`), persistent background daemon processes, memory exhaustion, CPU hogging, and disk-filling loops.
3. **Privilege escalation / Injection:** Malicious compiler argument injections or shell escapes.

Under [THREAT_MODEL.md §4.2](../../engineering/THREAT_MODEL.md#42-malicious-benchmark-programs-tb6-planned-m4), a timeout alone is **not** a sandbox. We need a principled, multi-platform isolation architecture that fails closed, labels every execution honestly, and operates without requiring root privileges.

## Decision

We adopt a defense-in-depth isolation architecture tailored per platform, implemented in Rust with zero shell invocation:

### 1. Linux Isolation Architecture

Where the kernel permits unprivileged namespaces, benchmarking runs in an isolated container combining:
- **Namespaces (`unshare`):**
  - **User namespace (`CLONE_NEWUSER`):** Maps the current UID/GID to root inside the namespace, eliminating host privileges.
  - **Mount namespace (`CLONE_NEWNS`):** Sets mount propagation to private (`MS_REC | MS_PRIVATE`), remounts the root file system read-only (`MS_RDONLY`), mounts an empty tmpfs over `/home` to conceal user home directories, masks `/etc/passwd` with a bind mount from `/dev/null`, and mounts a private writable tmpfs scratch directory for execution artifacts and temporary files.
  - **Network namespace (`CLONE_NEWNET`):** Disconnects all networking (loopback only, down by default), preventing exfiltration.
  - **PID namespace (`CLONE_NEWPID`):** Isolates process enumeration; when the benchmark child exits, all child/descendant processes in the namespace are automatically terminated by the kernel, preventing fork bombs and orphaned daemons.
- **Seccomp-BPF System Call Filtering & Argument Restrictions:**
  - **Architecture Validation (`AUDIT_ARCH`):** Validates native architecture (`AUDIT_ARCH_X86_64` or `AUDIT_ARCH_AARCH64`) at filter entry; terminates process (`SECCOMP_RET_KILL_PROCESS`) on architecture mismatch or if the x32 ABI bit (`0x40000000`) is asserted on x86_64.
  - **Execve / Execveat Restriction:** Denies `execve` unconditionally. Restricts `execveat` via BPF argument filtering: permits only `args[0] == 200` (`BENCH_BIN_FD`) and `args[4] & AT_EMPTY_PATH != 0`. The binary descriptor is opened and duped to fd 200 with `O_CLOEXEC` in `pre_exec`. `RLIMIT_NOFILE` is capped to 64 so that once fd 200 is closed upon execution, the child can never re-acquire fd 200. Any subsequent `execve` or `execveat` by the child is denied (`EPERM`).
  - **Network and Tracing Restriction:** Denies socket creation, bind, listen, connect, accept, accept4, and ptrace with `EPERM`.
  - **Async-Signal Safety:** The BPF instruction array is built before fork. `pre_exec` only installs the pre-built filter via a pointer/length `SockFprog`, performing zero heap allocations or locking.
- **POSIX Resource Limits (rlimits):**
  - `RLIMIT_CPU`: Hard CPU seconds limit; sends `SIGXCPU` then `SIGKILL`.
  - `RLIMIT_AS`: Hard virtual memory ceiling.
  - `RLIMIT_NPROC`: Best-effort per-real-UID ceiling; structural fork bomb containment is provided by `CLONE_NEWPID`.
  - `RLIMIT_FSIZE`: Hard file output size cap (e.g. 10 MiB) to prevent disk filling.
  - `RLIMIT_NOFILE`: Capped file descriptor ceiling.
- **Process Group Termination:**
  - The runner calls `setpgid(0, 0)` on the child.
  - On timeout or error, the runner sends `SIGKILL` to `-pgid`. When the child exits normally, post-reap signals are omitted to avoid races with pid recycling.

### 2. Windows Resource Limits (Job Objects)

On Windows, full container/sandbox isolation is not available to unprivileged CLI tools. The runner employs native Windows Job Objects for resource limits only:
- **Windows Job Object (`CreateJobObjectW`):**
  - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: Guarantees child termination when the handle closes.
  - `JOB_OBJECT_LIMIT_ACTIVE_PROCESS`: Enforces process creation limit.
  - `JOB_OBJECT_LIMIT_PROCESS_MEMORY` and `JOB_OBJECT_LIMIT_JOB_MEMORY`: Enforces memory caps.
  - `JOB_OBJECT_LIMIT_JOB_TIME`: Enforces user-mode CPU time limits.
- **Honest Labeling:** Windows Job Objects provide process resource limits, not sandbox isolation. The runner explicitly reports `isolation_level: "process-limits-only"` with `isolated: false` and emits a warning (`"not sandboxed: Windows Job Objects provide process resource limits only"`). It fails closed with `isolation_unavailable` unless `allow_unsandboxed: true` is passed.

### 3. macOS Isolation Strategy

- Modern macOS has deprecated `sandbox-exec` (`seatbelt`) and does not offer unprivileged container namespaces to CLI developer tools.
- On macOS, the runner applies `posix_spawn` with rlimits (`RLIMIT_CPU`, `RLIMIT_AS`, `RLIMIT_FSIZE`) and CPU timers.
- **Transparency Rule:** The runner labels macOS executions as `isolation_level: "process-limits-only"` and emits an explicit warning (`"not sandboxed"`).

### 4. Fail-Closed Policy & Explicit User Override

If full isolation cannot be established on Linux (e.g. unprivileged user namespaces disabled in `/proc/sys/kernel/unprivileged_userns_clone`), the engine:
1. **Fails closed by default:** Refuses to execute untrusted code and returns a structured error: `isolation_unavailable`.
2. **Explicit Override (`allow_unsandboxed: true`):** Runs only when the client request explicitly opts in, marking the response with `isolated: false` and `warnings: ["benchmark executed without sandbox isolation"]`.

### 5. Compiler & Runner Safety

- **No Shell:** Executables and compilers are spawned directly with fixed argument arrays (`Command::new("...")`).
- **Validated Options:** Optimization level (`-O0`, `-O2`, `-O3`) and language standards come from validated enums. Free-form compiler flags from requests are strictly rejected.
- **Fixed Source Names & Path Traversal Immunity:** Source content from requests is written to a fixed filename `solution.cpp` within an auto-cleaned temporary directory created via `tempfile` with mode 0700. `source.path` is treated as a display name only and mapped in diagnostics.
- **Flag Escapes & Operands:** Source file operands are preceded by `--` for Clang; for GCC (which rejects `--`), operands are prefixed with `./` to prevent filenames beginning with `-` from being parsed as flags.
- **Compilation Limits & Resource Caps:** Compilations run with a 15-second wall-clock timeout and a 512 KiB output size limit with process-group `SIGKILL` termination. Resource-capping compiler flags are enforced: `-ftemplate-depth=256`, `-fconstexpr-depth=256`, `-fconstexpr-steps=1048576` / `-fconstexpr-ops-limit=10000000`, and `-fmax-errors=10`.
- **Diagnostic Path Redaction:** Absolute host paths outside the compile scratch directory are redacted to `[redacted_path]` in compiler diagnostics and stderr to mitigate arbitrary file path disclosure.
- **Minimal Environment:** Compilers run with cleared environments (`cmd.env_clear()`), passing explicit `PATH` only (and required Windows environment variables on Windows) to prevent ambient `LD_PRELOAD` or include path overrides.

## Consequences

- Untrusted code cannot read sensitive files, make network connections, or spawn persistent background processes on supported systems.
- Benchmarking results are completely transparent about the level of isolation applied.
- Provides a clean foundation for BL-402 (compiler adapter), BL-403 (input generators), and BL-404-BL-407 (runner & protocol).
