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
- **Namespaces (`unshare` / `clone3`):**
  - **User namespace (`CLONE_NEWUSER`):** Maps the current UID/GID to root inside the namespace, eliminating host privileges.
  - **Mount namespace (`CLONE_NEWNS`):** Binds the root file system read-only (`MS_RDONLY`), mounts a restricted tmpfs scratch directory for input and output, and drops access to user home directories.
  - **Network namespace (`CLONE_NEWNET`):** Disconnects all networking (loopback only, down by default), preventing exfiltration.
  - **PID namespace (`CLONE_NEWPID`):** Isolates process enumeration; when the root process exits, all children are automatically terminated.
- **Seccomp-BPF System Call Allow-List:**
  - Allows only essential compute syscalls (`read`, `write`, `exit_group`, `brk`, `mmap`, `munmap`, `rt_sigreturn`, `clock_gettime`, `getrusage`).
  - Denies/traps `execve`, `fork`, `clone` (without worker isolation), `socket`, `connect`, `ptrace`, `kill`, `open` outside permitted descriptors.
- **POSIX Resource Limits (rlimits):**
  - `RLIMIT_CPU`: Hard CPU seconds limit; sends `SIGXCPU` then `SIGKILL`.
  - `RLIMIT_AS` / cgroup: Hard virtual memory ceiling.
  - `RLIMIT_NPROC`: Active process ceiling ($N=1$ or minimal) to prevent fork bombs.
  - `RLIMIT_FSIZE`: Hard file output size cap (e.g. 10 MiB) to prevent disk filling.
  - `RLIMIT_NOFILE`: File descriptor ceiling.
- **Process Group Termination:**
  - The runner calls `setpgid(0, 0)` on the child.
  - On exit, timeout, or cancellation, the runner sends `SIGKILL` to `-pgid`, reaping the entire process tree.

### 2. Windows Isolation Architecture

On Windows, the runner employs native Windows kernel isolation:
- **Windows Job Object (`CreateJobObjectW`):**
  - `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`: Guarantees child termination when the handle closes.
  - `JOB_OBJECT_LIMIT_ACTIVE_PROCESS = 1`: Prevents process creation/fork bombs.
  - `JOB_OBJECT_LIMIT_PROCESS_MEMORY` and `JOB_OBJECT_LIMIT_JOB_MEMORY`: Enforces memory caps.
  - `JOB_OBJECT_LIMIT_JOB_TIME`: Enforces user-mode CPU time limits.
- **Restricted Token (`CreateRestrictedToken`):**
  - Strips administrative and sensitive SIDs (`SeDebugPrivilege`, etc.).
  - Applies restricted SIDs and denies access to network tokens.

### 3. macOS Isolation Strategy

- Modern macOS has deprecated `sandbox-exec` (`seatbelt`) and does not offer unprivileged container namespaces to CLI developer tools.
- On macOS, the runner applies `posix_spawn` with rlimits (`RLIMIT_CPU`, `RLIMIT_AS`, `RLIMIT_FSIZE`) and CPU timers.
- **Transparency Rule:** The runner labels macOS executions as `isolation_level: "process-limits-only"` and emits an explicit warning (`"not sandboxed"`).

### 4. Fail-Closed Policy & Explicit User Override

If full isolation cannot be established on Linux (e.g. unprivileged user namespaces disabled in `/proc/sys/kernel/unprivileged_userns_clone`), the engine:
1. **Fails closed by default:** Refuses to execute untrusted code and returns a structured error: `isolation_unavailable`.
2. **Explicit Override (`allow_unsandboxed: true`):** Runs only when the client request explicitly opts in, marking the response with `isolated: false` and `warnings: ["benchmark executed without sandbox isolation"]`.

### 5. Compiler & Runner Safety

- **No Shell:** Executables are spawned directly with fixed argument arrays (`Command::new("...")`).
- **Validated Options:** Optimization level (`-O0`, `-O2`, `-O3`) and language standards come from validated enums.
- **Flag Escapes (`--`):** Source file operands are preceded by `--` to prevent source files named `-fplugin=...` from being interpreted as flags.
- **RAII Scratch Directories:** All compilation artifacts, inputs, and outputs reside in per-run temporary directories that are wiped upon completion.

## Consequences

- Untrusted code cannot read sensitive files, make network connections, or spawn persistent background processes on supported systems.
- Benchmarking results are completely transparent about the level of isolation applied.
- Provides a clean foundation for BL-402 (compiler adapter), BL-403 (input generators), and BL-404-BL-407 (runner & protocol).
