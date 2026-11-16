//! Process runner with timeouts, resource limits (rlimits / Windows Job Objects),
//! process groups, and peak-RSS measurement.
//!
//! Follows ADR 0014, BL-404, and BL-406.

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Execution status of a benchmark run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkRunStatus {
    /// Program completed with exit code 0 within all resource bounds.
    Success,
    /// Program exceeded wall-clock or CPU timeout and was terminated.
    Timeout,
    /// Program exceeded the virtual memory ceiling (`RLIMIT_AS`).
    MemoryLimitExceeded,
    /// Program exceeded the maximum allowed output size (`RLIMIT_FSIZE`).
    OutputLimitExceeded,
    /// Program terminated abnormally (e.g. fatal signal or non-zero exit).
    Crash(String),
    /// Runner encountered an unexpected operating system or spawning error.
    Error(String),
}

/// Resource constraints for child process execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerLimits {
    /// Maximum elapsed wall-clock duration before killing the process tree.
    pub wall_timeout: Duration,
    /// Maximum CPU execution time (soft limit via `RLIMIT_CPU`).
    pub cpu_timeout: Duration,
    /// Maximum virtual address space in bytes (`RLIMIT_AS`).
    pub max_memory_bytes: u64,
    /// Maximum output file size in bytes (`RLIMIT_FSIZE`).
    pub max_output_bytes: u64,
    /// Maximum process count ceiling (`RLIMIT_NPROC`), if configured.
    pub max_nproc: Option<u64>,
    /// Maximum open file descriptors (`RLIMIT_NOFILE`).
    pub max_nofile: u64,
    /// Whether to enforce strong namespace and seccomp sandbox isolation (default: true).
    pub isolate: bool,
    /// Whether to allow fallback to process-limits-only when isolation is unavailable (default: false).
    pub allow_unsandboxed: bool,
}

impl Default for RunnerLimits {
    fn default() -> Self {
        Self {
            wall_timeout: Duration::from_millis(1000),
            cpu_timeout: Duration::from_millis(1000),
            max_memory_bytes: 256 * 1024 * 1024, // 256 MiB
            max_output_bytes: 10 * 1024 * 1024,  // 10 MiB
            max_nproc: None,
            max_nofile: 256,
            isolate: true,
            allow_unsandboxed: false,
        }
    }
}

/// Result of a single execution of a benchmark target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SingleRunResult {
    /// Final execution status.
    pub status: BenchmarkRunStatus,
    /// Measured wall-clock duration.
    pub wall_time: Duration,
    /// Measured user + system CPU time from `rusage`.
    pub cpu_time: Duration,
    /// Measured peak RSS in bytes.
    pub peak_rss_bytes: u64,
    /// Captured standard output (capped at `max_output_bytes`).
    pub stdout: Vec<u8>,
    /// Captured standard error (capped at 64 KiB).
    pub stderr: String,
}

/// Robust summary of a series of repeated runs at a given input size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSampleResult {
    /// Overall status of the sample series.
    pub status: BenchmarkRunStatus,
    /// Statistical summary of wall-clock times (ms), if successful.
    pub wall_time_summary: Option<bigolens_core::statistics::SampleSummary>,
    /// Statistical summary of CPU times (ms), if successful.
    pub cpu_time_summary: Option<bigolens_core::statistics::SampleSummary>,
    /// Highest peak RSS observed across all runs (bytes).
    pub peak_rss_bytes: u64,
    /// Raw measured wall-clock times (ms).
    pub raw_wall_times_ms: Vec<f64>,
    /// Raw measured CPU times (ms).
    pub raw_cpu_times_ms: Vec<f64>,
    /// Applied isolation level string.
    pub isolation_level: &'static str,
    /// Whether strong sandbox isolation was enforced (false until BL-405).
    pub isolated: bool,
    /// User-facing warnings regarding isolation and execution conditions.
    pub warnings: Vec<String>,
}

#[cfg(unix)]
struct ProcessGroupGuard {
    pgid: libc::pid_t,
    active: bool,
}

#[cfg(unix)]
impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                libc::kill(-self.pgid, libc::SIGKILL);
                let mut status = 0;
                while libc::waitpid(-self.pgid, &mut status, libc::WNOHANG) > 0 {}
            }
        }
    }
}

/// Execute a single run of the given binary with resource limits and stdin input.
#[cfg(unix)]
pub fn execute_single_run(
    executable: &Path,
    input: &[u8],
    limits: &RunnerLimits,
) -> SingleRunResult {
    use std::os::unix::process::CommandExt;

    let mut cmd = Command::new(executable);
    if let Some(parent) = executable.parent() {
        cmd.current_dir(parent);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let max_mem = limits.max_memory_bytes;
    let cpu_sec = limits.cpu_timeout.as_secs().max(1);
    let max_out = limits.max_output_bytes;
    let max_nofile = limits.max_nofile;
    let max_nproc = limits.max_nproc;

    let isolation_available = crate::isolation::is_linux_isolation_available();
    if limits.isolate && !isolation_available && !limits.allow_unsandboxed {
        return SingleRunResult {
            status: BenchmarkRunStatus::Error(
                "isolation_unavailable: unprivileged namespaces or seccomp are required for dynamic benchmarks (pass allow_unsandboxed: true to override)".to_string(),
            ),
            wall_time: Duration::ZERO,
            cpu_time: Duration::ZERO,
            peak_rss_bytes: 0,
            stdout: Vec::new(),
            stderr: String::new(),
        };
    }

    let should_isolate = limits.isolate && isolation_available;
    let scratch_dir = executable.parent().map(|p| p.to_path_buf());
    let bin_path_c = std::ffi::CString::new(executable.to_str().unwrap_or("")).unwrap_or_default();

    #[cfg(target_os = "linux")]
    const BENCH_BIN_FD: libc::c_int = 200;

    #[cfg(target_os = "linux")]
    let (bin_fd, seccomp_prog) = if should_isolate {
        let fd = unsafe { libc::open(bin_path_c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) };
        if fd < 0 {
            return SingleRunResult {
                status: BenchmarkRunStatus::Error(format!(
                    "failed to open binary for isolation: {}",
                    std::io::Error::last_os_error()
                )),
                wall_time: Duration::ZERO,
                cpu_time: Duration::ZERO,
                peak_rss_bytes: 0,
                stdout: Vec::new(),
                stderr: String::new(),
            };
        }
        let prog = match crate::isolation::build_seccomp_filter(BENCH_BIN_FD) {
            Ok(p) => p,
            Err(e) => {
                unsafe { libc::close(fd) };
                return SingleRunResult {
                    status: BenchmarkRunStatus::Error(format!(
                        "failed to build seccomp filter: {}",
                        e
                    )),
                    wall_time: Duration::ZERO,
                    cpu_time: Duration::ZERO,
                    peak_rss_bytes: 0,
                    stdout: Vec::new(),
                    stderr: String::new(),
                };
            }
        };
        (fd, Some(prog))
    } else {
        (-1, None)
    };

    #[cfg(target_os = "linux")]
    let fprog_opt = seccomp_prog.as_ref().map(|p| p.as_fprog());

    unsafe {
        cmd.pre_exec(move || {
            // 1. Place child into its own process group (pgid == child pid)
            if libc::setpgid(0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }

            // 2. CPU time limit
            let cpu_rlim = libc::rlimit {
                rlim_cur: cpu_sec,
                rlim_max: cpu_sec + 1,
            };
            libc::setrlimit(libc::RLIMIT_CPU, &cpu_rlim);

            // 3. Virtual address space limit
            if max_mem > 0 {
                let as_rlim = libc::rlimit {
                    rlim_cur: max_mem,
                    rlim_max: max_mem,
                };
                libc::setrlimit(libc::RLIMIT_AS, &as_rlim);
            }

            // 4. Output file size limit
            if max_out > 0 {
                let fsize_rlim = libc::rlimit {
                    rlim_cur: max_out,
                    rlim_max: max_out,
                };
                libc::setrlimit(libc::RLIMIT_FSIZE, &fsize_rlim);
            }

            // 5. If isolated, dup benchmark binary to BENCH_BIN_FD before setting RLIMIT_NOFILE
            #[cfg(target_os = "linux")]
            if should_isolate && bin_fd >= 0 {
                if libc::dup3(bin_fd, BENCH_BIN_FD, libc::O_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                libc::close(bin_fd);
            }

            // 6. Open file descriptors limit (capped at 64 in isolated mode so BENCH_BIN_FD=200 cannot be re-opened)
            if max_nofile > 0 {
                let limit_val = if should_isolate {
                    max_nofile.min(64)
                } else {
                    max_nofile
                };
                let nofile_rlim = libc::rlimit {
                    rlim_cur: limit_val,
                    rlim_max: limit_val,
                };
                libc::setrlimit(libc::RLIMIT_NOFILE, &nofile_rlim);
            }

            // 7. Max process ceiling if configured
            if let Some(nproc) = max_nproc {
                let nproc_rlim = libc::rlimit {
                    rlim_cur: nproc,
                    rlim_max: nproc,
                };
                libc::setrlimit(libc::RLIMIT_NPROC, &nproc_rlim);
            }

            // 8. Apply unprivileged Linux namespaces and seccomp if requested
            #[cfg(target_os = "linux")]
            if should_isolate {
                crate::isolation::apply_linux_namespaces(scratch_dir.as_deref())?;

                let child = libc::fork();
                if child < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if child == 0 {
                    // Benchmark child: runs as PID 1 in the new PID namespace
                    libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0);

                    // Mount fresh /proc for the new PID namespace
                    let proc_target = c"/proc".as_ptr();
                    let proc_type = c"proc".as_ptr();
                    let proc_flags =
                        libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC | libc::MS_RDONLY;
                    let opts = c"hidepid=2".as_ptr() as *const libc::c_void;
                    if libc::mount(proc_type, proc_target, proc_type, proc_flags, opts) != 0
                        && libc::mount(
                            proc_type,
                            proc_target,
                            proc_type,
                            proc_flags,
                            std::ptr::null(),
                        ) != 0
                    {
                        let msg = b"ISOLATION_FAILURE: mount /proc failed\n";
                        libc::write(
                            libc::STDERR_FILENO,
                            msg.as_ptr() as *const libc::c_void,
                            msg.len(),
                        );
                        libc::_exit(125);
                    }

                    // Drop all capabilities inside user namespace
                    if crate::isolation::drop_all_capabilities().is_err() {
                        let msg = b"ISOLATION_FAILURE: drop capabilities failed\n";
                        libc::write(
                            libc::STDERR_FILENO,
                            msg.as_ptr() as *const libc::c_void,
                            msg.len(),
                        );
                        libc::_exit(124);
                    }

                    if let Some(ref fp) = fprog_opt {
                        if crate::isolation::apply_seccomp_filter(fp).is_err() {
                            let msg = b"ISOLATION_FAILURE: seccomp filter failed\n";
                            libc::write(
                                libc::STDERR_FILENO,
                                msg.as_ptr() as *const libc::c_void,
                                msg.len(),
                            );
                            libc::_exit(126);
                        }
                    }

                    let empty = c"".as_ptr();
                    let argv: [*const libc::c_char; 2] = [bin_path_c.as_ptr(), std::ptr::null()];
                    let envp: [*const libc::c_char; 1] = [std::ptr::null()];
                    libc::syscall(
                        libc::SYS_execveat,
                        BENCH_BIN_FD,
                        empty,
                        argv.as_ptr(),
                        envp.as_ptr(),
                        libc::AT_EMPTY_PATH,
                    );
                    let msg = b"ISOLATION_FAILURE: execveat failed\n";
                    libc::write(
                        libc::STDERR_FILENO,
                        msg.as_ptr() as *const libc::c_void,
                        msg.len(),
                    );
                    libc::_exit(127);
                }

                // Intermediate launcher process:
                // Close all file descriptors other than stdio (0, 1, 2).
                // This closes the Rust standard library's internal spawn error pipe,
                // unblocking cmd.spawn() in the runner immediately.
                for fd in 3..BENCH_BIN_FD {
                    libc::close(fd);
                }
                libc::close(BENCH_BIN_FD);
                for fd in (BENCH_BIN_FD + 1)..1024 {
                    libc::close(fd);
                }

                let mut status = 0;
                let mut rusage: libc::rusage = std::mem::zeroed();
                let res = libc::wait4(child, &mut status, 0, &mut rusage);
                if res < 0 {
                    libc::_exit(1);
                }
                if libc::WIFEXITED(status) {
                    libc::_exit(libc::WEXITSTATUS(status));
                } else if libc::WIFSIGNALED(status) {
                    let sig = libc::WTERMSIG(status);
                    libc::signal(sig, libc::SIG_DFL);
                    libc::raise(sig);
                    libc::_exit(128 + sig);
                }
                libc::_exit(1);
            }

            Ok(())
        });
    }

    let mut child = match cmd.spawn() {
        Ok(c) => {
            #[cfg(target_os = "linux")]
            if bin_fd >= 0 {
                unsafe { libc::close(bin_fd) };
            }
            c
        }
        Err(e) => {
            #[cfg(target_os = "linux")]
            if bin_fd >= 0 {
                unsafe { libc::close(bin_fd) };
            }
            return SingleRunResult {
                status: BenchmarkRunStatus::Error(format!("spawn failed: {}", e)),
                wall_time: Duration::ZERO,
                cpu_time: Duration::ZERO,
                peak_rss_bytes: 0,
                stdout: Vec::new(),
                stderr: String::new(),
            };
        }
    };

    let pid = child.id() as libc::pid_t;
    let pgid = pid;
    let mut guard = ProcessGroupGuard { pgid, active: true };

    // Asynchronously write stdin to prevent pipe deadlock
    if let Some(mut stdin) = child.stdin.take() {
        let input_vec = input.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input_vec);
        });
    }

    // Asynchronously read stdout up to byte limit
    let stdout_pipe = child.stdout.take();
    let stdout_cap = limits.max_output_bytes;
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stream) = stdout_pipe {
            let mut chunk = [0u8; 8192];
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if buf.len() + n > stdout_cap as usize {
                    let rem = (stdout_cap as usize).saturating_sub(buf.len());
                    buf.extend_from_slice(&chunk[..rem]);
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        buf
    });

    // Asynchronously read stderr up to 64 KiB
    let stderr_pipe = child.stderr.take();
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(stream) = stderr_pipe {
            let mut handle = stream.take(65536);
            let _ = handle.read_to_string(&mut buf);
        }
        buf
    });

    // Monitor child process execution and wall-clock timeout
    let start = Instant::now();
    let mut timed_out = false;
    let mut status: libc::c_int = 0;
    let mut rusage: libc::rusage = unsafe { std::mem::zeroed() };

    loop {
        let res = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut rusage) };
        if res == pid {
            break;
        } else if res == -1 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ECHILD) {
                break;
            }
        }

        if start.elapsed() >= limits.wall_timeout {
            timed_out = true;
            unsafe {
                libc::kill(-pgid, libc::SIGKILL);
                libc::wait4(pid, &mut status, 0, &mut rusage);
            }
            break;
        }

        std::thread::sleep(Duration::from_millis(1));
    }

    let elapsed = start.elapsed();

    // Ensure all descendant processes in this group are terminated only if timed out.
    // Avoid racing against pgid reuse when the child has already cleanly exited.
    if timed_out {
        unsafe {
            libc::kill(-pgid, libc::SIGKILL);
            let mut dummy = 0;
            while libc::waitpid(-pgid, &mut dummy, libc::WNOHANG) > 0 {}
        }
    }
    guard.active = false;

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_str = stderr_thread.join().unwrap_or_default();

    let utime = Duration::from_secs(rusage.ru_utime.tv_sec as u64)
        + Duration::from_micros(rusage.ru_utime.tv_usec as u64);
    let stime = Duration::from_secs(rusage.ru_stime.tv_sec as u64)
        + Duration::from_micros(rusage.ru_stime.tv_usec as u64);
    let cpu_time = utime + stime;

    #[cfg(target_os = "linux")]
    let peak_rss_bytes = (rusage.ru_maxrss as u64) * 1024;
    #[cfg(target_os = "macos")]
    let peak_rss_bytes = rusage.ru_maxrss as u64;
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let peak_rss_bytes = 0;

    let final_status = if timed_out {
        BenchmarkRunStatus::Timeout
    } else if stderr_str.contains("ISOLATION_FAILURE") {
        BenchmarkRunStatus::Error(stderr_str.trim().to_string())
    } else if libc::WIFSIGNALED(status) {
        let sig = libc::WTERMSIG(status);
        if sig == libc::SIGXCPU {
            BenchmarkRunStatus::Timeout
        } else if sig == libc::SIGXFSZ {
            BenchmarkRunStatus::OutputLimitExceeded
        } else if sig == libc::SIGSEGV || sig == libc::SIGBUS {
            if stderr_str.contains("bad_alloc") || stderr_str.contains("out of memory") {
                BenchmarkRunStatus::MemoryLimitExceeded
            } else {
                BenchmarkRunStatus::Crash(format!("signal {} (SIGSEGV)", sig))
            }
        } else if sig == libc::SIGABRT {
            if stderr_str.contains("bad_alloc") || stderr_str.contains("out of memory") {
                BenchmarkRunStatus::MemoryLimitExceeded
            } else {
                BenchmarkRunStatus::Crash("signal 6 (SIGABRT)".to_string())
            }
        } else if sig == libc::SIGFPE {
            BenchmarkRunStatus::Crash("signal 8 (SIGFPE)".to_string())
        } else if sig == libc::SIGKILL {
            BenchmarkRunStatus::Timeout
        } else {
            BenchmarkRunStatus::Crash(format!("signal {}", sig))
        }
    } else if libc::WIFEXITED(status) {
        let code = libc::WEXITSTATUS(status);
        if code == 0 {
            BenchmarkRunStatus::Success
        } else if stderr_str.contains("bad_alloc") || stderr_str.contains("out of memory") {
            BenchmarkRunStatus::MemoryLimitExceeded
        } else {
            BenchmarkRunStatus::Crash(format!("exit code {}", code))
        }
    } else {
        BenchmarkRunStatus::Error("unknown child process exit status".to_string())
    };

    SingleRunResult {
        status: final_status,
        wall_time: elapsed,
        cpu_time,
        peak_rss_bytes,
        stdout: stdout_bytes,
        stderr: stderr_str,
    }
}

/// Prepares Job Object limit flags and accounting configuration from `RunnerLimits`.
pub fn build_job_limits_config(limits: &RunnerLimits) -> (u32, i64, usize, Option<u32>) {
    let mut flags = win32_constants::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | win32_constants::JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;

    let cpu_100ns = if limits.cpu_timeout > Duration::ZERO {
        flags |= win32_constants::JOB_OBJECT_LIMIT_JOB_TIME;
        (limits.cpu_timeout.as_nanos() / 100) as i64
    } else {
        0
    };

    let max_mem = if limits.max_memory_bytes > 0 {
        flags |= win32_constants::JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | win32_constants::JOB_OBJECT_LIMIT_JOB_MEMORY;
        limits.max_memory_bytes as usize
    } else {
        0
    };

    let max_nproc = if let Some(nproc) = limits.max_nproc {
        flags |= win32_constants::JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
        Some(nproc as u32)
    } else {
        None
    };

    (flags, cpu_100ns, max_mem, max_nproc)
}

/// Constants for Windows Job Objects, process status codes, and synchronization.
#[allow(
    non_snake_case,
    non_camel_case_types,
    non_upper_case_globals,
    dead_code
)]
pub mod win32_constants {
    pub const CREATE_SUSPENDED: u32 = 0x0000_0004;

    pub const JOB_OBJECT_LIMIT_JOB_TIME: u32 = 0x0000_0004;
    pub const JOB_OBJECT_LIMIT_ACTIVE_PROCESS: u32 = 0x0000_0008;
    pub const JOB_OBJECT_LIMIT_PROCESS_MEMORY: u32 = 0x0000_0100;
    pub const JOB_OBJECT_LIMIT_JOB_MEMORY: u32 = 0x0000_0200;
    pub const JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION: u32 = 0x0000_0400;
    pub const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    pub const JobObjectBasicAccountingInformation: u32 = 1;
    pub const JobObjectExtendedLimitInformation: u32 = 9;

    pub const WAIT_OBJECT_0: u32 = 0x0000_0000;
    pub const WAIT_TIMEOUT: u32 = 0x0000_0102;
    pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

    pub const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
    pub const THREAD_SUSPEND_RESUME: u32 = 0x0002;

    pub const STATUS_SUCCESS: u32 = 0x0000_0000;
    pub const STATUS_JOB_OBJECT_LIMIT_REACHED: u32 = 0xC000_0412;
    pub const STATUS_NO_MEMORY: u32 = 0xC000_0017;
    pub const STATUS_COMMITMENT_LIMIT: u32 = 0xC000_0128;
    pub const STATUS_ACCESS_VIOLATION: u32 = 0xC000_0005;
    pub const STATUS_STACK_OVERFLOW: u32 = 0xC000_00FD;
    pub const STATUS_ILLEGAL_INSTRUCTION: u32 = 0xC000_001D;
    pub const STATUS_FLOAT_DIVIDE_BY_ZERO: u32 = 0xC000_008E;
    pub const STATUS_INTEGER_DIVIDE_BY_ZERO: u32 = 0xC000_0094;
    pub const STATUS_CONTROL_C_EXIT: u32 = 0xC000_013A;
    pub const STATUS_DLL_NOT_FOUND: u32 = 0xC000_0135;
    pub const STATUS_ENTRYPOINT_NOT_FOUND: u32 = 0xC000_0139;
}

pub use win32_constants::*;

/// Map a Windows process exit code, stderr output, timeout condition, and CPU time
/// to a normalized `BenchmarkRunStatus`.
pub fn map_windows_exit_status(
    raw_exit_code: u32,
    stderr_str: &str,
    timed_out: bool,
    cpu_time: Duration,
    limits: &RunnerLimits,
) -> BenchmarkRunStatus {
    if timed_out {
        return BenchmarkRunStatus::Timeout;
    }

    if limits.cpu_timeout > Duration::ZERO && cpu_time >= limits.cpu_timeout {
        return BenchmarkRunStatus::Timeout;
    }

    if stderr_str.contains("bad_alloc")
        || stderr_str.contains("out of memory")
        || stderr_str.contains("std::bad_alloc")
    {
        return BenchmarkRunStatus::MemoryLimitExceeded;
    }

    if raw_exit_code == win32_constants::STATUS_JOB_OBJECT_LIMIT_REACHED {
        if limits.cpu_timeout > Duration::ZERO && cpu_time >= limits.cpu_timeout {
            return BenchmarkRunStatus::Timeout;
        }
        return BenchmarkRunStatus::MemoryLimitExceeded;
    }

    if raw_exit_code == win32_constants::STATUS_NO_MEMORY
        || raw_exit_code == win32_constants::STATUS_COMMITMENT_LIMIT
    {
        return BenchmarkRunStatus::MemoryLimitExceeded;
    }

    if raw_exit_code == win32_constants::STATUS_SUCCESS {
        return BenchmarkRunStatus::Success;
    }

    match raw_exit_code {
        win32_constants::STATUS_ACCESS_VIOLATION => {
            BenchmarkRunStatus::Crash("access violation (0xC0000005)".to_string())
        }
        win32_constants::STATUS_STACK_OVERFLOW => {
            BenchmarkRunStatus::Crash("stack overflow (0xC00000FD)".to_string())
        }
        win32_constants::STATUS_ILLEGAL_INSTRUCTION => {
            BenchmarkRunStatus::Crash("illegal instruction (0xC000001D)".to_string())
        }
        win32_constants::STATUS_FLOAT_DIVIDE_BY_ZERO
        | win32_constants::STATUS_INTEGER_DIVIDE_BY_ZERO => {
            BenchmarkRunStatus::Crash("divide by zero (0xC0000094)".to_string())
        }
        win32_constants::STATUS_CONTROL_C_EXIT => {
            BenchmarkRunStatus::Crash("terminated by user/control-c (0xC000013A)".to_string())
        }
        win32_constants::STATUS_DLL_NOT_FOUND => BenchmarkRunStatus::Error(
            "dynamic-link library (DLL) not found (0xC0000135)".to_string(),
        ),
        win32_constants::STATUS_ENTRYPOINT_NOT_FOUND => {
            BenchmarkRunStatus::Error("entrypoint not found (0xC0000139)".to_string())
        }
        code if (code & 0xC000_0000) == 0xC000_0000 => {
            BenchmarkRunStatus::Crash(format!("fatal exception 0x{:08X}", code))
        }
        code => BenchmarkRunStatus::Crash(format!("exit code {}", code)),
    }
}

#[cfg(windows)]
#[allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
pub mod win32 {
    use super::*;
    use std::ffi::c_void;
    use std::io;

    pub type HANDLE = *mut c_void;
    pub type BOOL = i32;
    pub type DWORD = u32;
    pub type ULONG_PTR = usize;
    pub type LARGE_INTEGER = i64;

    pub const FALSE: BOOL = 0;
    pub const NULL_HANDLE: HANDLE = std::ptr::null_mut();
    pub const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct IO_COUNTERS {
        pub ReadOperationCount: u64,
        pub WriteOperationCount: u64,
        pub OtherOperationCount: u64,
        pub ReadTransferCount: u64,
        pub WriteTransferCount: u64,
        pub OtherTransferCount: u64,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        pub PerProcessUserTimeLimit: LARGE_INTEGER,
        pub PerJobUserTimeLimit: LARGE_INTEGER,
        pub LimitFlags: DWORD,
        pub MinimumWorkingSetSize: ULONG_PTR,
        pub MaximumWorkingSetSize: ULONG_PTR,
        pub ActiveProcessLimit: DWORD,
        pub Affinity: ULONG_PTR,
        pub PriorityClass: DWORD,
        pub SchedulingClass: DWORD,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        pub BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        pub IoInfo: IO_COUNTERS,
        pub ProcessMemoryLimit: ULONG_PTR,
        pub JobMemoryLimit: ULONG_PTR,
        pub PeakProcessMemoryLimit: ULONG_PTR,
        pub PeakJobMemoryLimit: ULONG_PTR,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct JOBOBJECT_BASIC_ACCOUNTING_INFORMATION {
        pub TotalUserTime: LARGE_INTEGER,
        pub TotalKernelTime: LARGE_INTEGER,
        pub ThisPeriodTotalUserTime: LARGE_INTEGER,
        pub ThisPeriodTotalKernelTime: LARGE_INTEGER,
        pub TotalPageFaultCount: DWORD,
        pub TotalProcesses: DWORD,
        pub ActiveProcesses: DWORD,
        pub TotalTerminatedProcesses: DWORD,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct PROCESS_MEMORY_COUNTERS {
        pub cb: DWORD,
        pub PageFaultCount: DWORD,
        pub PeakWorkingSetSize: ULONG_PTR,
        pub WorkingSetSize: ULONG_PTR,
        pub QuotaPeakPagedPoolUsage: ULONG_PTR,
        pub QuotaPagedPoolUsage: ULONG_PTR,
        pub QuotaPeakNonPagedPoolUsage: ULONG_PTR,
        pub QuotaNonPagedPoolUsage: ULONG_PTR,
        pub PagefileUsage: ULONG_PTR,
        pub PeakPagefileUsage: ULONG_PTR,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct THREADENTRY32 {
        pub dwSize: DWORD,
        pub cntUsage: DWORD,
        pub th32ThreadID: DWORD,
        pub th32OwnerProcessID: DWORD,
        pub tpBasePri: i32,
        pub tpDeltaPri: i32,
        pub dwFlags: DWORD,
    }

    #[repr(C)]
    #[derive(Debug, Default, Clone, Copy)]
    pub struct FILETIME {
        pub dwLowDateTime: DWORD,
        pub dwHighDateTime: DWORD,
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn CreateJobObjectW(lpJobAttributes: *mut c_void, lpName: *const u16) -> HANDLE;
        pub fn SetInformationJobObject(
            hJob: HANDLE,
            JobObjectInformationClass: u32,
            lpJobObjectInformation: *const c_void,
            cbJobObjectInformationLength: u32,
        ) -> BOOL;
        pub fn QueryInformationJobObject(
            hJob: HANDLE,
            JobObjectInformationClass: u32,
            lpJobObjectInformation: *mut c_void,
            cbJobObjectInformationLength: u32,
            lpReturnLength: *mut u32,
        ) -> BOOL;
        pub fn AssignProcessToJobObject(hJob: HANDLE, hProcess: HANDLE) -> BOOL;
        pub fn TerminateJobObject(hJob: HANDLE, uExitCode: u32) -> BOOL;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
        pub fn WaitForSingleObject(hHandle: HANDLE, dwMilliseconds: DWORD) -> DWORD;
        pub fn GetExitCodeProcess(hProcess: HANDLE, lpExitCode: *mut DWORD) -> BOOL;
        pub fn GetProcessTimes(
            hProcess: HANDLE,
            lpCreationTime: *mut FILETIME,
            lpExitTime: *mut FILETIME,
            lpKernelTime: *mut FILETIME,
            lpUserTime: *mut FILETIME,
        ) -> BOOL;
        pub fn CreateToolhelp32Snapshot(dwFlags: DWORD, th32ProcessID: DWORD) -> HANDLE;
        pub fn Thread32First(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
        pub fn Thread32Next(hSnapshot: HANDLE, lpte: *mut THREADENTRY32) -> BOOL;
        pub fn OpenThread(
            dwDesiredAccess: DWORD,
            bInheritHandle: BOOL,
            dwThreadId: DWORD,
        ) -> HANDLE;
        pub fn ResumeThread(hThread: HANDLE) -> DWORD;
    }

    #[link(name = "psapi")]
    extern "system" {
        pub fn GetProcessMemoryInfo(
            Process: HANDLE,
            ppsmemCounters: *mut PROCESS_MEMORY_COUNTERS,
            cb: DWORD,
        ) -> BOOL;
    }

    /// RAII drop guard for a Win32 Job Object handle.
    pub struct JobHandle {
        pub(crate) handle: HANDLE,
    }

    impl JobHandle {
        pub fn create() -> io::Result<Self> {
            // SAFETY: Calling CreateJobObjectW with null attributes creates an anonymous Job Object.
            let handle = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            Ok(Self { handle })
        }

        #[inline]
        pub fn raw(&self) -> HANDLE {
            self.handle
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            if !self.handle.is_null() && self.handle != INVALID_HANDLE_VALUE {
                // SAFETY: self.handle is a valid, owned Win32 Job Object handle.
                unsafe {
                    CloseHandle(self.handle);
                }
                self.handle = NULL_HANDLE;
            }
        }
    }

    /// Resumes all threads belonging to the target process ID.
    pub fn resume_process_threads(pid: u32) -> io::Result<()> {
        // SAFETY: Snapshot of all threads in the system.
        let snapshot = unsafe { CreateToolhelp32Snapshot(win32_constants::TH32CS_SNAPTHREAD, 0) };
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        struct SnapshotGuard(HANDLE);
        impl Drop for SnapshotGuard {
            fn drop(&mut self) {
                // SAFETY: Closing snapshot handle.
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
        let _guard = SnapshotGuard(snapshot);

        let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<THREADENTRY32>() as DWORD;

        // SAFETY: snapshot is valid and entry.dwSize is initialized.
        if unsafe { Thread32First(snapshot, &mut entry) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let mut resumed_any = false;
        loop {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: Opening thread with THREAD_SUSPEND_RESUME permissions.
                let h_thread = unsafe {
                    OpenThread(
                        win32_constants::THREAD_SUSPEND_RESUME,
                        FALSE,
                        entry.th32ThreadID,
                    )
                };
                if !h_thread.is_null() && h_thread != INVALID_HANDLE_VALUE {
                    // SAFETY: Calling ResumeThread on a valid thread handle.
                    let prev = unsafe { ResumeThread(h_thread) };
                    // SAFETY: Closing thread handle.
                    unsafe {
                        CloseHandle(h_thread);
                    }
                    if prev != DWORD::MAX {
                        resumed_any = true;
                    }
                }
            }

            entry.dwSize = std::mem::size_of::<THREADENTRY32>() as DWORD;
            // SAFETY: Advancing to next thread in snapshot.
            if unsafe { Thread32Next(snapshot, &mut entry) } == 0 {
                break;
            }
        }

        if resumed_any {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "no threads found to resume",
            ))
        }
    }
}

/// Execute a single run of the given binary with Windows Job Objects resource limits and stdin input.
#[cfg(windows)]
pub fn execute_single_run(
    executable: &Path,
    input: &[u8],
    limits: &RunnerLimits,
) -> SingleRunResult {
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use win32::*;

    if limits.isolate && !limits.allow_unsandboxed {
        return SingleRunResult {
            status: BenchmarkRunStatus::Error(
                "isolation_unavailable: Windows Job Objects provide process resource limits only, not sandbox isolation (pass allow_unsandboxed: true to override)".to_string(),
            ),
            wall_time: Duration::ZERO,
            cpu_time: Duration::ZERO,
            peak_rss_bytes: 0,
            stdout: Vec::new(),
            stderr: String::new(),
        };
    }

    let mut cmd = Command::new(executable);
    if let Some(parent) = executable.parent() {
        cmd.current_dir(parent);
    }
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // 1. Configure child process to start in a suspended state
    cmd.creation_flags(win32_constants::CREATE_SUSPENDED);

    // 2. Create Win32 Job Object with RAII drop guard
    let job = match JobHandle::create() {
        Ok(j) => j,
        Err(e) => {
            return SingleRunResult {
                status: BenchmarkRunStatus::Error(format!("CreateJobObjectW failed: {}", e)),
                wall_time: Duration::ZERO,
                cpu_time: Duration::ZERO,
                peak_rss_bytes: 0,
                stdout: Vec::new(),
                stderr: String::new(),
            };
        }
    };

    // 3. Set extended limits on the Job Object
    let (limit_flags, cpu_100ns, max_mem, max_nproc) = build_job_limits_config(limits);
    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    info.BasicLimitInformation.LimitFlags = limit_flags;
    info.BasicLimitInformation.PerJobUserTimeLimit = cpu_100ns;
    info.ProcessMemoryLimit = max_mem;
    info.JobMemoryLimit = max_mem;
    if let Some(nproc) = max_nproc {
        info.BasicLimitInformation.ActiveProcessLimit = nproc;
    }

    // SAFETY: job.raw() is a valid handle and info has correct size and layout.
    let set_info_ok = unsafe {
        SetInformationJobObject(
            job.raw(),
            win32_constants::JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };

    if set_info_ok == 0 {
        let err = std::io::Error::last_os_error();
        return SingleRunResult {
            status: BenchmarkRunStatus::Error(format!("SetInformationJobObject failed: {}", err)),
            wall_time: Duration::ZERO,
            cpu_time: Duration::ZERO,
            peak_rss_bytes: 0,
            stdout: Vec::new(),
            stderr: String::new(),
        };
    }

    // 4. Spawn child suspended
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return SingleRunResult {
                status: BenchmarkRunStatus::Error(format!("spawn failed: {}", e)),
                wall_time: Duration::ZERO,
                cpu_time: Duration::ZERO,
                peak_rss_bytes: 0,
                stdout: Vec::new(),
                stderr: String::new(),
            };
        }
    };

    let pid = child.id();
    let h_process = child.as_raw_handle() as HANDLE;

    // 5. Assign child process to Job Object
    // SAFETY: job.raw() and h_process are valid handles.
    let assign_ok = unsafe { AssignProcessToJobObject(job.raw(), h_process) };
    if assign_ok == 0 {
        let err = std::io::Error::last_os_error();
        let _ = child.kill();
        return SingleRunResult {
            status: BenchmarkRunStatus::Error(format!("AssignProcessToJobObject failed: {}", err)),
            wall_time: Duration::ZERO,
            cpu_time: Duration::ZERO,
            peak_rss_bytes: 0,
            stdout: Vec::new(),
            stderr: String::new(),
        };
    }

    // 6. Resume child process thread
    if let Err(e) = resume_process_threads(pid) {
        let _ = child.kill();
        return SingleRunResult {
            status: BenchmarkRunStatus::Error(format!("ResumeThread failed: {}", e)),
            wall_time: Duration::ZERO,
            cpu_time: Duration::ZERO,
            peak_rss_bytes: 0,
            stdout: Vec::new(),
            stderr: String::new(),
        };
    }

    // 7. Asynchronously write stdin to prevent pipe deadlock
    if let Some(mut stdin) = child.stdin.take() {
        let input_vec = input.to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&input_vec);
        });
    }

    // 8. Asynchronously read stdout up to byte limit
    let stdout_pipe = child.stdout.take();
    let stdout_cap = limits.max_output_bytes;
    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stream) = stdout_pipe {
            let mut chunk = [0u8; 8192];
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if buf.len() + n > stdout_cap as usize {
                    let rem = (stdout_cap as usize).saturating_sub(buf.len());
                    buf.extend_from_slice(&chunk[..rem]);
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        buf
    });

    // 9. Asynchronously read stderr up to 64 KiB
    let stderr_pipe = child.stderr.take();
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(stream) = stderr_pipe {
            let mut handle = stream.take(65536);
            let _ = handle.read_to_string(&mut buf);
        }
        buf
    });

    // 10. Wait for process with wall-clock timeout
    let start = Instant::now();
    let timeout_ms = limits
        .wall_timeout
        .as_millis()
        .min(win32_constants::WAIT_FAILED as u128) as u32;

    // SAFETY: h_process is a valid handle to the child process.
    let wait_res = unsafe { WaitForSingleObject(h_process, timeout_ms) };
    let mut timed_out = false;

    if wait_res == win32_constants::WAIT_TIMEOUT {
        timed_out = true;
        // Terminate the entire job object immediately
        // SAFETY: job.raw() is a valid handle.
        unsafe {
            TerminateJobObject(job.raw(), win32_constants::STATUS_JOB_OBJECT_LIMIT_REACHED);
            WaitForSingleObject(h_process, 1000);
        }
    }

    let elapsed = start.elapsed();

    // Collect child pipes output
    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_str = stderr_thread.join().unwrap_or_default();

    // 11. Query CPU time and peak RSS
    let mut raw_exit_code: u32 = 0;
    // SAFETY: h_process is valid and raw_exit_code is a valid pointer.
    unsafe {
        GetExitCodeProcess(h_process, &mut raw_exit_code);
    }

    let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
    let mut ret_len: u32 = 0;
    // SAFETY: job.raw() is valid and accounting pointer is valid.
    let query_acc_ok = unsafe {
        QueryInformationJobObject(
            job.raw(),
            win32_constants::JobObjectBasicAccountingInformation,
            &mut accounting as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            &mut ret_len,
        )
    };

    let cpu_time = if query_acc_ok != 0 {
        let total_100ns = accounting
            .TotalUserTime
            .saturating_add(accounting.TotalKernelTime);
        if total_100ns > 0 {
            Duration::from_nanos((total_100ns as u64).saturating_mul(100))
        } else {
            Duration::ZERO
        }
    } else {
        let mut creation: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit: FILETIME = unsafe { std::mem::zeroed() };
        let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
        let mut user: FILETIME = unsafe { std::mem::zeroed() };
        // SAFETY: h_process is valid.
        if unsafe { GetProcessTimes(h_process, &mut creation, &mut exit, &mut kernel, &mut user) }
            != 0
        {
            let user_100ns = ((user.dwHighDateTime as u64) << 32) | (user.dwLowDateTime as u64);
            let kernel_100ns =
                ((kernel.dwHighDateTime as u64) << 32) | (kernel.dwLowDateTime as u64);
            Duration::from_nanos(user_100ns.saturating_add(kernel_100ns).saturating_mul(100))
        } else {
            Duration::ZERO
        }
    };

    let mut ext_info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: job.raw() is valid and ext_info pointer is valid.
    let query_ext_ok = unsafe {
        QueryInformationJobObject(
            job.raw(),
            win32_constants::JobObjectExtendedLimitInformation,
            &mut ext_info as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            &mut ret_len,
        )
    };

    let mut peak_rss_bytes = if query_ext_ok != 0 {
        ext_info
            .PeakProcessMemoryLimit
            .max(ext_info.PeakJobMemoryLimit) as u64
    } else {
        0
    };

    if peak_rss_bytes == 0 {
        let mut pmc: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
        pmc.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        // SAFETY: h_process is valid.
        if unsafe { GetProcessMemoryInfo(h_process, &mut pmc, pmc.cb) } != 0 {
            peak_rss_bytes = pmc.PeakWorkingSetSize as u64;
        }
    }

    // 12. Map execution status
    let status = map_windows_exit_status(raw_exit_code, &stderr_str, timed_out, cpu_time, limits);

    SingleRunResult {
        status,
        wall_time: elapsed,
        cpu_time,
        peak_rss_bytes,
        stdout: stdout_bytes,
        stderr: stderr_str,
    }
}

/// Fallback runner execution for operating systems without native runner support.
#[cfg(not(any(unix, windows)))]
pub fn execute_single_run(
    _executable: &Path,
    _input: &[u8],
    _limits: &RunnerLimits,
) -> SingleRunResult {
    fallback_unsupported_run()
}

#[allow(dead_code)]
pub(crate) fn fallback_unsupported_run() -> SingleRunResult {
    SingleRunResult {
        status: BenchmarkRunStatus::Error("runner not implemented for target OS".to_string()),
        wall_time: Duration::ZERO,
        cpu_time: Duration::ZERO,
        peak_rss_bytes: 0,
        stdout: Vec::new(),
        stderr: String::new(),
    }
}

/// Run a series of warmup and repeated measurements, summarizing the results.
pub fn run_benchmark_series(
    executable: &Path,
    input: &[u8],
    limits: &RunnerLimits,
    warmup: usize,
    repetitions: usize,
) -> BenchmarkSampleResult {
    #[cfg(target_os = "linux")]
    let isolation_available = crate::isolation::is_linux_isolation_available();
    #[cfg(not(target_os = "linux"))]
    let isolation_available = false;

    let is_isolated = limits.isolate && isolation_available;

    if limits.isolate && !isolation_available && !limits.allow_unsandboxed {
        return BenchmarkSampleResult {
            status: BenchmarkRunStatus::Error(
                "isolation_unavailable: unprivileged namespaces or seccomp are required for dynamic benchmarks (pass allow_unsandboxed: true to override)".to_string(),
            ),
            wall_time_summary: None,
            cpu_time_summary: None,
            peak_rss_bytes: 0,
            raw_wall_times_ms: Vec::new(),
            raw_cpu_times_ms: Vec::new(),
            isolation_level: "none",
            isolated: false,
            warnings: vec!["isolation unavailable: failed closed".to_string()],
        };
    }

    #[cfg(target_os = "linux")]
    let (isolation_level, warnings) = if is_isolated {
        ("linux-namespaces-seccomp", Vec::new())
    } else {
        (
            "process-limits-only",
            vec!["not sandboxed: running with process resource limits only".to_string()],
        )
    };

    #[cfg(windows)]
    let (isolation_level, warnings) = (
        "process-limits-only",
        vec!["not sandboxed: Windows Job Objects provide process resource limits only".to_string()],
    );

    #[cfg(not(any(target_os = "linux", windows)))]
    let (isolation_level, warnings) = (
        "process-limits-only",
        vec!["not sandboxed: running with process resource limits only".to_string()],
    );

    #[cfg(not(any(target_os = "linux", windows)))]
    let (isolation_level, warnings) = (
        "process-limits-only",
        vec!["not sandboxed: running with process resource limits only".to_string()],
    );

    // Warm-up runs (unmeasured)
    for _ in 0..warmup {
        let run = execute_single_run(executable, input, limits);
        if run.status != BenchmarkRunStatus::Success {
            return BenchmarkSampleResult {
                status: run.status,
                wall_time_summary: None,
                cpu_time_summary: None,
                peak_rss_bytes: run.peak_rss_bytes,
                raw_wall_times_ms: Vec::new(),
                raw_cpu_times_ms: Vec::new(),
                isolation_level,
                isolated: is_isolated,
                warnings,
            };
        }
    }

    // Repeated runs
    let mut raw_wall_times_ms = Vec::with_capacity(repetitions);
    let mut raw_cpu_times_ms = Vec::with_capacity(repetitions);
    let mut max_peak_rss = 0u64;

    for _ in 0..repetitions {
        let run = execute_single_run(executable, input, limits);
        if run.status != BenchmarkRunStatus::Success {
            return BenchmarkSampleResult {
                status: run.status,
                wall_time_summary: None,
                cpu_time_summary: None,
                peak_rss_bytes: run.peak_rss_bytes.max(max_peak_rss),
                raw_wall_times_ms,
                raw_cpu_times_ms,
                isolation_level,
                isolated: is_isolated,
                warnings,
            };
        }
        raw_wall_times_ms.push(run.wall_time.as_secs_f64() * 1000.0);
        raw_cpu_times_ms.push(run.cpu_time.as_secs_f64() * 1000.0);
        if run.peak_rss_bytes > max_peak_rss {
            max_peak_rss = run.peak_rss_bytes;
        }
    }

    let wall_summary = bigolens_core::statistics::summarize(&raw_wall_times_ms);
    let cpu_summary = bigolens_core::statistics::summarize(&raw_cpu_times_ms);

    BenchmarkSampleResult {
        status: BenchmarkRunStatus::Success,
        wall_time_summary: wall_summary,
        cpu_time_summary: cpu_summary,
        peak_rss_bytes: max_peak_rss,
        raw_wall_times_ms,
        raw_cpu_times_ms,
        isolation_level,
        isolated: is_isolated,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{compile_cpp, discover_compiler, CppStandard, OptimizationLevel};
    use std::fs;

    struct TestDir(tempfile::TempDir);

    impl std::ops::Deref for TestDir {
        type Target = std::path::Path;
        fn deref(&self) -> &std::path::Path {
            self.0.path()
        }
    }

    impl AsRef<std::path::Path> for TestDir {
        fn as_ref(&self) -> &std::path::Path {
            self.0.path()
        }
    }

    fn make_test_dir(name: &str) -> TestDir {
        TestDir(
            tempfile::Builder::new()
                .prefix(&format!("bigolens_runner_test_{}_", name))
                .tempdir()
                .expect("failed to create test directory"),
        )
    }

    #[test]
    #[cfg(unix)]
    fn test_normal_program_execution() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return, // Skip test if no compiler installed
        };
        let work_dir = make_test_dir("normal");
        let source = r#"
#include <iostream>
int main() {
    long long n;
    if (std::cin >> n) {
        std::cout << (n * 2) << "\n";
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            wall_timeout: Duration::from_millis(3000),
            allow_unsandboxed: true,
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"21\n", &limits);

        assert_eq!(res.status, BenchmarkRunStatus::Success);
        let out_str = String::from_utf8_lossy(&res.stdout);
        assert_eq!(out_str.trim(), "42");
        assert!(res.wall_time > Duration::ZERO);

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_infinite_loop_killed_at_timeout() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("infloop");
        let source = r#"
int main() {
    volatile int x = 0;
    while (true) {
        x++;
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            wall_timeout: Duration::from_millis(100),
            cpu_timeout: Duration::from_millis(100),
            allow_unsandboxed: true,
            ..Default::default()
        };

        let res = execute_single_run(&bin, b"", &limits);
        assert_eq!(res.status, BenchmarkRunStatus::Timeout);

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_memory_limit_exceeded() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("memlimit");
        let source = r#"
#include <vector>
int main() {
    std::vector<char> v(200 * 1024 * 1024);
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            max_memory_bytes: 50 * 1024 * 1024,
            wall_timeout: Duration::from_millis(3000),
            allow_unsandboxed: true,
            ..Default::default()
        };

        let res = execute_single_run(&bin, b"", &limits);
        assert_eq!(res.status, BenchmarkRunStatus::MemoryLimitExceeded);

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_file_size_limit_exceeded() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("filesize");
        let source = r#"
#include <fstream>
int main() {
    std::ofstream out("big_test_file.bin");
    for (int i = 0; i < 200000; ++i) {
        out << "0123456789012345678901234567890123456789\n";
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            max_output_bytes: 50 * 1024,
            wall_timeout: Duration::from_millis(3000),
            allow_unsandboxed: true,
            isolate: false,
            ..Default::default()
        };

        let res = execute_single_run(&bin, b"", &limits);
        assert_eq!(res.status, BenchmarkRunStatus::OutputLimitExceeded);

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_crash_signal_reported() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("crash");
        let source = r#"
int main() {
    int* ptr = nullptr;
    *ptr = 42;
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            wall_timeout: Duration::from_millis(3000),
            allow_unsandboxed: true,
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);

        match res.status {
            BenchmarkRunStatus::Crash(msg) => {
                assert!(msg.contains("SIGSEGV") || msg.contains("signal 11"));
            }
            other => panic!("expected Crash status, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_fork_bomb_contained_and_cleaned() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("forkbomb");
        let source = r#"
#include <unistd.h>
int main() {
    while (true) {
        fork();
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            wall_timeout: Duration::from_millis(100),
            cpu_timeout: Duration::from_millis(100),
            allow_unsandboxed: true,
            ..Default::default()
        };

        let res = execute_single_run(&bin, b"", &limits);
        assert!(
            res.status == BenchmarkRunStatus::Timeout
                || matches!(res.status, BenchmarkRunStatus::Crash(_)),
            "unexpected status: {:?}",
            res.status
        );

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_benchmark_series_warmup_and_repetition() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("series");
        let source = r#"
#include <iostream>
int main() {
    long long n;
    if (std::cin >> n) {
        long long sum = 0;
        for (long long i = 0; i < n; ++i) {
            sum += i;
        }
        std::cout << sum << "\n";
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: false,
            ..Default::default()
        };
        let sample = run_benchmark_series(&bin, b"100000\n", &limits, 1, 5);

        assert_eq!(sample.status, BenchmarkRunStatus::Success);
        assert!(!sample.isolated);
        assert_eq!(sample.isolation_level, "process-limits-only");
        assert!(sample.warnings.iter().any(|w| w.contains("not sandboxed")));
        assert_eq!(sample.raw_wall_times_ms.len(), 5);

        let summary = sample.wall_time_summary.expect("summary should exist");
        assert_eq!(summary.count, 5);
        assert!(summary.median > 0.0);

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_reading_home_files() {
        if !crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_home");
        let source = r#"
#include <fstream>
#include <iostream>
int main() {
    std::ifstream f("/home");
    // Under unshare mount namespace, /home is an empty tmpfs with no user subdirectories
    std::ifstream f2("/home/rifz/.bashrc");
    if (f2.is_open()) {
        std::cout << "LEAKED_HOME\n";
        return 0;
    }
    std::cerr << "HOME_CONCEALED\n";
    return 1;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("HOME_CONCEALED"));
        let out = String::from_utf8_lossy(&res.stdout);
        assert!(!out.contains("LEAKED_HOME"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_socket_creation() {
        if !crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_socket");
        let source = r#"
#include <sys/socket.h>
#include <iostream>
int main() {
    int s = socket(AF_INET, SOCK_STREAM, 0);
    if (s < 0) {
        std::cerr << "SOCKET_BLOCKED\n";
        return 1;
    }
    std::cout << "SOCKET_ALLOWED\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("SOCKET_BLOCKED"));
        let out = String::from_utf8_lossy(&res.stdout);
        assert!(!out.contains("SOCKET_ALLOWED"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_execve() {
        if !crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_execve");
        let source = r#"
#include <unistd.h>
#include <iostream>
int main() {
    char* argv[] = {(char*)"/bin/echo", (char*)"escaped", nullptr};
    execv("/bin/echo", argv);
    std::cerr << "EXECVE_BLOCKED\n";
    return 1;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("EXECVE_BLOCKED"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_ptrace() {
        if !crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_ptrace");
        let source = r#"
#include <sys/ptrace.h>
#include <iostream>
int main() {
    long ret = ptrace(PTRACE_TRACEME, 0, nullptr, nullptr);
    if (ret < 0) {
        std::cerr << "PTRACE_BLOCKED\n";
        return 1;
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("PTRACE_BLOCKED"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_execveat() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_execveat");
        let source = r#"
#include <fcntl.h>
#include <unistd.h>
#include <sys/syscall.h>
#include <iostream>
int main() {
    char* argv[] = {(char*)"/bin/echo", (char*)"escaped", nullptr};
    char* envp[] = {nullptr};
    long ret = syscall(SYS_execveat, AT_FDCWD, "/bin/echo", argv, envp, 0);
    if (ret < 0) {
        std::cerr << "EXECVEAT_BLOCKED\n";
        return 1;
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("EXECVEAT_BLOCKED"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_writing_outside_scratch() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_outside_write");
        let source = r#"
#include <fstream>
#include <iostream>
int main() {
    std::ofstream f("/tmp/bigolens_escape_test.txt");
    if (f.is_open()) {
        std::cerr << "WRITE_OUTSIDE_ALLOWED\n";
        return 0;
    }
    std::ofstream f_local("local_scratch.txt");
    if (!f_local.is_open()) {
        std::cerr << "LOCAL_WRITE_FAILED\n";
        return 2;
    }
    f_local << "scratch";
    f_local.close();
    std::cerr << "WRITE_OUTSIDE_BLOCKED\n";
    return 1;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("WRITE_OUTSIDE_BLOCKED"));
        assert!(!res.stderr.contains("WRITE_OUTSIDE_ALLOWED"));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_reading_etc_passwd() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_passwd");
        let source = r#"
#include <fstream>
#include <iostream>
#include <string>
int main() {
    std::ifstream f("/etc/passwd");
    if (!f.is_open()) {
        std::cout << "PASSWD_UNREADABLE\n";
        return 0;
    }
    std::string line;
    if (std::getline(f, line)) {
        if (line.find("root:") != std::string::npos) {
            std::cerr << "PASSWD_EXPOSED\n";
            return 1;
        }
    }
    std::cout << "PASSWD_EMPTY_OR_CONCEALED\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        let stdout = String::from_utf8_lossy(&res.stdout);
        assert!(!res.stderr.contains("PASSWD_EXPOSED"));
        assert!(
            stdout.contains("PASSWD_EMPTY_OR_CONCEALED") || stdout.contains("PASSWD_UNREADABLE")
        );

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn test_sandbox_kills_x32_abi_invocation() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_x32");
        let source = r#"
#include <sys/syscall.h>
#include <unistd.h>
#include <iostream>
int main() {
    long ret = syscall(0x40000000 | SYS_getpid);
    std::cout << "X32_ALLOWED: " << ret << "\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        match res.status {
            BenchmarkRunStatus::Crash(ref msg) => {
                assert!(msg.contains("SIGSYS") || msg.contains("signal"));
            }
            other => panic!("expected Crash due to SIGSYS/kill, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_isolated_series_success() {
        if !crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("iso_series");
        let source = r#"
#include <iostream>
int main() {
    long long n;
    if (std::cin >> n) {
        std::cout << (n * 3) << "\n";
    }
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let sample = run_benchmark_series(&bin, b"10\n", &limits, 1, 3);
        assert_eq!(sample.status, BenchmarkRunStatus::Success);
        assert!(sample.isolated);
        assert_eq!(sample.isolation_level, "linux-namespaces-seccomp");
        assert!(sample.warnings.is_empty());

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_pid_namespace_isolates_pids() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_proc_pids");
        let source = r#"
#include <iostream>
#include <fstream>
#include <string>
#include <unistd.h>
#include <dirent.h>

int main() {
    pid_t pid = getpid();
    if (pid != 1) {
        std::cerr << "NOT_PID_1: " << pid << "\n";
        return 1;
    }
    std::ifstream status_file("/proc/self/status");
    std::string line;
    bool nspid_found = false;
    while (std::getline(status_file, line)) {
        if (line.rfind("NSpid:", 0) == 0) {
            nspid_found = true;
            if (line.find("1", 6) == std::string::npos) {
                std::cerr << "INVALID_NSPID: " << line << "\n";
                return 2;
            }
        }
    }
    if (!nspid_found) {
        std::cerr << "NSPID_NOT_FOUND\n";
        return 3;
    }
    DIR* dir = opendir("/proc");
    if (!dir) {
        std::cerr << "OPENDIR_PROC_FAILED\n";
        return 4;
    }
    struct dirent* entry;
    int numeric_dirs = 0;
    while ((entry = readdir(dir)) != nullptr) {
        if (entry->d_name[0] >= '0' && entry->d_name[0] <= '9') {
            numeric_dirs++;
            if (std::string(entry->d_name) != "1") {
                std::cerr << "HOST_PID_VISIBLE: " << entry->d_name << "\n";
                closedir(dir);
                return 5;
            }
        }
    }
    closedir(dir);
    if (numeric_dirs != 1) {
        std::cerr << "UNEXPECTED_PID_COUNT: " << numeric_dirs << "\n";
        return 6;
    }
    std::cout << "PID_ISOLATION_VERIFIED\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        let stdout = String::from_utf8_lossy(&res.stdout);
        assert_eq!(
            res.status,
            BenchmarkRunStatus::Success,
            "Execution failed: stderr={}",
            res.stderr
        );
        assert!(
            stdout.contains("PID_ISOLATION_VERIFIED"),
            "Output missing verification: stdout={}, stderr={}",
            stdout,
            res.stderr
        );

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_isolated_rusage_accounting() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("isolated_rusage");
        let source = r#"
#include <iostream>
#include <vector>
#include <chrono>

int main() {
    // 1. Allocate and touch 30 MB to verify peak RSS accounting across launcher shim
    const size_t bytes = 30 * 1024 * 1024;
    std::vector<char> v(bytes, 42);
    volatile char sink = 0;
    for (size_t i = 0; i < bytes; i += 4096) {
        sink += v[i];
    }

    // 2. Burn CPU for at least 50 ms to verify CPU time accounting across launcher shim
    auto start = std::chrono::steady_clock::now();
    long long iter = 0;
    while (std::chrono::duration_cast<std::chrono::milliseconds>(
               std::chrono::steady_clock::now() - start).count() < 50) {
        iter++;
    }

    std::cout << "OK:" << iter << "\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_eq!(
            res.status,
            BenchmarkRunStatus::Success,
            "Execution failed: stderr={}",
            res.stderr
        );
        assert!(
            res.cpu_time >= Duration::from_millis(10),
            "Expected cpu_time >= 10ms, got {:?}",
            res.cpu_time
        );
        assert!(
            res.peak_rss_bytes >= 20 * 1024 * 1024,
            "Expected peak_rss_bytes >= 20MB, got {}",
            res.peak_rss_bytes
        );

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_sandbox_denies_privilege_escalation_and_filesystem_unmasking() {
        if !crate::isolation::is_linux_isolation_available() {
            println!("Skipping test: Linux isolation is unavailable in this environment");
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("deny_priv_esc");
        let source = r#"
#include <iostream>
#include <fstream>
#include <string>
#include <sys/mount.h>
#include <sched.h>
#include <unistd.h>
#include <sys/syscall.h>

int main() {
    // 1. Try umount2 on /home
    if (umount2("/home", MNT_DETACH) == 0) {
        std::cerr << "ESCAPE_ALLOWED: umount2 /home succeeded\n";
        return 1;
    }
    // 2. Try umount2 on /etc/passwd
    if (umount2("/etc/passwd", MNT_DETACH) == 0) {
        std::cerr << "ESCAPE_ALLOWED: umount2 /etc/passwd succeeded\n";
        return 2;
    }
    // 3. Try mount
    if (mount("tmpfs", "/tmp", "tmpfs", 0, nullptr) == 0) {
        std::cerr << "ESCAPE_ALLOWED: mount succeeded\n";
        return 3;
    }
    // 4. Try unshare
    if (unshare(CLONE_NEWUSER) == 0) {
        std::cerr << "ESCAPE_ALLOWED: unshare CLONE_NEWUSER succeeded\n";
        return 4;
    }
    // 5. Try chroot
    if (chroot("/") == 0) {
        std::cerr << "ESCAPE_ALLOWED: chroot succeeded\n";
        return 5;
    }

    // Verify /etc/passwd cannot be read
    std::ifstream passwd("/etc/passwd");
    std::string line;
    if (passwd.is_open() && std::getline(passwd, line)) {
        if (line.find("root:") != std::string::npos) {
            std::cerr << "PASSWD_EXPOSED\n";
            return 6;
        }
    }

    std::cout << "CAPABILITIES_AND_MASKING_SECURE\n";
    return 0;
}
"#;
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            wall_timeout: Duration::from_secs(5),
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        let stdout = String::from_utf8_lossy(&res.stdout);
        assert_eq!(
            res.status,
            BenchmarkRunStatus::Success,
            "Execution failed: stderr={}",
            res.stderr
        );
        assert!(
            stdout.contains("CAPABILITIES_AND_MASKING_SECURE"),
            "Output missing verification: stdout={}, stderr={}",
            stdout,
            res.stderr
        );

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_unsandboxed_override_warning() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("unsandboxed_warn");
        let source = "int main() { return 0; }";
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: false,
            allow_unsandboxed: true,
            ..Default::default()
        };
        let res = run_benchmark_series(&bin, b"", &limits, 0, 1);
        assert_eq!(res.status, BenchmarkRunStatus::Success);
        assert!(!res.isolated);
        assert_eq!(res.isolation_level, "process-limits-only");
        assert!(res.warnings.iter().any(|w| w.contains("not sandboxed")));

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    #[cfg(unix)]
    fn test_fail_closed_when_isolation_unavailable() {
        if crate::isolation::is_linux_isolation_available() {
            return;
        }
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };
        let work_dir = make_test_dir("fail_closed");
        let source = "int main() { return 0; }";
        let bin = compile_cpp(
            &compiler,
            source,
            "solution.cpp",
            &work_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O0,
        )
        .expect("compilation failed");

        let limits = RunnerLimits {
            isolate: true,
            allow_unsandboxed: false,
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        match res.status {
            BenchmarkRunStatus::Error(msg) => {
                assert!(msg.contains("isolation_unavailable"));
            }
            other => panic!("expected isolation_unavailable error, got {:?}", other),
        }

        let sample = run_benchmark_series(&bin, b"", &limits, 0, 1);
        match sample.status {
            BenchmarkRunStatus::Error(msg) => {
                assert!(msg.contains("isolation_unavailable"));
            }
            other => panic!("expected isolation_unavailable error, got {:?}", other),
        }

        let _ = fs::remove_dir_all(&work_dir);
    }

    #[test]
    fn test_windows_job_limits_config_defaults() {
        let limits = RunnerLimits::default();
        let (flags, cpu_100ns, max_mem, max_nproc) = build_job_limits_config(&limits);

        // Required flags: kill on close, die on unhandled exception, job time, process memory, job memory
        assert_eq!(
            flags & win32_constants::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            win32_constants::JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        );
        assert_eq!(
            flags & win32_constants::JOB_OBJECT_LIMIT_PROCESS_MEMORY,
            win32_constants::JOB_OBJECT_LIMIT_PROCESS_MEMORY
        );
        assert_eq!(
            flags & win32_constants::JOB_OBJECT_LIMIT_JOB_MEMORY,
            win32_constants::JOB_OBJECT_LIMIT_JOB_MEMORY
        );
        assert_eq!(
            flags & win32_constants::JOB_OBJECT_LIMIT_JOB_TIME,
            win32_constants::JOB_OBJECT_LIMIT_JOB_TIME
        );
        assert_eq!(flags & win32_constants::JOB_OBJECT_LIMIT_ACTIVE_PROCESS, 0);

        // 1000ms = 1s = 10_000_000 in 100ns units
        assert_eq!(cpu_100ns, 10_000_000);
        // Default 256 MiB
        assert_eq!(max_mem, 256 * 1024 * 1024);
        assert_eq!(max_nproc, None);
    }

    #[test]
    fn test_windows_job_limits_config_custom_limits() {
        let limits = RunnerLimits {
            wall_timeout: Duration::from_millis(500),
            cpu_timeout: Duration::from_millis(250),
            max_memory_bytes: 64 * 1024 * 1024,
            max_nproc: Some(3),
            ..Default::default()
        };
        let (flags, cpu_100ns, max_mem, max_nproc) = build_job_limits_config(&limits);

        assert_eq!(
            flags & win32_constants::JOB_OBJECT_LIMIT_ACTIVE_PROCESS,
            win32_constants::JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        );
        assert_eq!(cpu_100ns, 2_500_000);
        assert_eq!(max_mem, 64 * 1024 * 1024);
        assert_eq!(max_nproc, Some(3));
    }

    #[test]
    fn test_windows_job_limits_config_zero_limits() {
        let limits = RunnerLimits {
            cpu_timeout: Duration::ZERO,
            max_memory_bytes: 0,
            max_nproc: None,
            ..Default::default()
        };
        let (flags, cpu_100ns, max_mem, max_nproc) = build_job_limits_config(&limits);

        assert_eq!(flags & win32_constants::JOB_OBJECT_LIMIT_JOB_TIME, 0);
        assert_eq!(flags & win32_constants::JOB_OBJECT_LIMIT_PROCESS_MEMORY, 0);
        assert_eq!(flags & win32_constants::JOB_OBJECT_LIMIT_ACTIVE_PROCESS, 0);
        assert_eq!(cpu_100ns, 0);
        assert_eq!(max_mem, 0);
        assert_eq!(max_nproc, None);
    }

    #[test]
    fn test_windows_status_mapping_success() {
        let limits = RunnerLimits::default();
        let status = map_windows_exit_status(
            win32_constants::STATUS_SUCCESS,
            "",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(status, BenchmarkRunStatus::Success);
    }

    #[test]
    fn test_windows_status_mapping_timeout_conditions() {
        let limits = RunnerLimits {
            cpu_timeout: Duration::from_millis(500),
            ..Default::default()
        };

        // Wall-clock timeout flag
        let s1 = map_windows_exit_status(
            win32_constants::STATUS_SUCCESS,
            "",
            true,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s1, BenchmarkRunStatus::Timeout);

        // CPU timeout reached
        let s2 = map_windows_exit_status(
            win32_constants::STATUS_SUCCESS,
            "",
            false,
            Duration::from_millis(600),
            &limits,
        );
        assert_eq!(s2, BenchmarkRunStatus::Timeout);

        // Job Object limit reached with CPU time exceeding limit
        let s3 = map_windows_exit_status(
            win32_constants::STATUS_JOB_OBJECT_LIMIT_REACHED,
            "",
            false,
            Duration::from_millis(500),
            &limits,
        );
        assert_eq!(s3, BenchmarkRunStatus::Timeout);
    }

    #[test]
    fn test_windows_status_mapping_memory_exhaustion() {
        let limits = RunnerLimits {
            cpu_timeout: Duration::from_millis(5000),
            ..Default::default()
        };

        // Job object limit reached (memory limit)
        let s1 = map_windows_exit_status(
            win32_constants::STATUS_JOB_OBJECT_LIMIT_REACHED,
            "",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s1, BenchmarkRunStatus::MemoryLimitExceeded);

        // STATUS_NO_MEMORY (0xC0000017)
        let s2 = map_windows_exit_status(
            win32_constants::STATUS_NO_MEMORY,
            "",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s2, BenchmarkRunStatus::MemoryLimitExceeded);

        // STATUS_COMMITMENT_LIMIT (0xC0000128)
        let s3 = map_windows_exit_status(
            win32_constants::STATUS_COMMITMENT_LIMIT,
            "",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s3, BenchmarkRunStatus::MemoryLimitExceeded);

        // Non-zero exit with std::bad_alloc in stderr
        let s4 = map_windows_exit_status(
            1,
            "terminate called after throwing an instance of 'std::bad_alloc'",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s4, BenchmarkRunStatus::MemoryLimitExceeded);

        // Non-zero exit with out of memory in stderr
        let s5 = map_windows_exit_status(
            1,
            "fatal error: out of memory allocating 100MB",
            false,
            Duration::from_millis(100),
            &limits,
        );
        assert_eq!(s5, BenchmarkRunStatus::MemoryLimitExceeded);
    }

    #[test]
    fn test_windows_status_mapping_fatal_exceptions() {
        let limits = RunnerLimits::default();

        let s_av = map_windows_exit_status(
            win32_constants::STATUS_ACCESS_VIOLATION,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_av,
            BenchmarkRunStatus::Crash("access violation (0xC0000005)".to_string())
        );

        let s_so = map_windows_exit_status(
            win32_constants::STATUS_STACK_OVERFLOW,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_so,
            BenchmarkRunStatus::Crash("stack overflow (0xC00000FD)".to_string())
        );

        let s_ii = map_windows_exit_status(
            win32_constants::STATUS_ILLEGAL_INSTRUCTION,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_ii,
            BenchmarkRunStatus::Crash("illegal instruction (0xC000001D)".to_string())
        );

        let s_dz = map_windows_exit_status(
            win32_constants::STATUS_INTEGER_DIVIDE_BY_ZERO,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_dz,
            BenchmarkRunStatus::Crash("divide by zero (0xC0000094)".to_string())
        );

        let s_ctrlc = map_windows_exit_status(
            win32_constants::STATUS_CONTROL_C_EXIT,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_ctrlc,
            BenchmarkRunStatus::Crash("terminated by user/control-c (0xC000013A)".to_string())
        );

        let s_generic_nt = map_windows_exit_status(0xC000_0001, "", false, Duration::ZERO, &limits);
        assert_eq!(
            s_generic_nt,
            BenchmarkRunStatus::Crash("fatal exception 0xC0000001".to_string())
        );
    }

    #[test]
    fn test_windows_status_mapping_dll_and_entrypoint_errors() {
        let limits = RunnerLimits::default();

        let s_dll = map_windows_exit_status(
            win32_constants::STATUS_DLL_NOT_FOUND,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_dll,
            BenchmarkRunStatus::Error(
                "dynamic-link library (DLL) not found (0xC0000135)".to_string()
            )
        );

        let s_entry = map_windows_exit_status(
            win32_constants::STATUS_ENTRYPOINT_NOT_FOUND,
            "",
            false,
            Duration::ZERO,
            &limits,
        );
        assert_eq!(
            s_entry,
            BenchmarkRunStatus::Error("entrypoint not found (0xC0000139)".to_string())
        );
    }

    #[test]
    fn test_windows_status_mapping_custom_exit_code() {
        let limits = RunnerLimits::default();
        let s = map_windows_exit_status(42, "", false, Duration::ZERO, &limits);
        assert_eq!(s, BenchmarkRunStatus::Crash("exit code 42".to_string()));
    }

    #[test]
    fn test_mock_fallback_runner_unsupported_os() {
        let res = fallback_unsupported_run();
        assert_eq!(
            res.status,
            BenchmarkRunStatus::Error("runner not implemented for target OS".to_string())
        );
        assert_eq!(res.wall_time, Duration::ZERO);
        assert_eq!(res.cpu_time, Duration::ZERO);
        assert_eq!(res.peak_rss_bytes, 0);
        assert!(res.stdout.is_empty());
        assert!(res.stderr.is_empty());
    }
}
