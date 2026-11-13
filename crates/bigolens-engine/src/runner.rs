//! Process runner with timeouts, resource limits (rlimits), process groups,
//! and peak-RSS measurement.
//!
//! Follows ADR 0014 and BL-404.

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

    let bin_path_c = std::ffi::CString::new(executable.to_str().unwrap_or("")).unwrap_or_default();
    let bin_fd = if should_isolate {
        unsafe { libc::open(bin_path_c.as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC) }
    } else {
        -1
    };

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

            // 5. Open file descriptors limit
            if max_nofile > 0 {
                let nofile_rlim = libc::rlimit {
                    rlim_cur: max_nofile,
                    rlim_max: max_nofile,
                };
                libc::setrlimit(libc::RLIMIT_NOFILE, &nofile_rlim);
            }

            // 6. Max process ceiling if configured
            if let Some(nproc) = max_nproc {
                let nproc_rlim = libc::rlimit {
                    rlim_cur: nproc,
                    rlim_max: nproc,
                };
                libc::setrlimit(libc::RLIMIT_NPROC, &nproc_rlim);
            }

            // 7. Apply unprivileged Linux namespaces and seccomp if requested
            #[cfg(target_os = "linux")]
            if should_isolate {
                crate::isolation::apply_linux_namespaces()?;
                crate::isolation::apply_seccomp_filter()?;

                if bin_fd >= 0 {
                    let empty = c"".as_ptr();
                    let argv: [*const libc::c_char; 2] = [bin_path_c.as_ptr(), std::ptr::null()];
                    let envp: [*const libc::c_char; 1] = [std::ptr::null()];
                    libc::syscall(
                        libc::SYS_execveat,
                        bin_fd,
                        empty,
                        argv.as_ptr(),
                        envp.as_ptr(),
                        libc::AT_EMPTY_PATH,
                    );
                    return Err(std::io::Error::last_os_error());
                }
            }

            Ok(())
        });
    }

    let mut child = match cmd.spawn() {
        Ok(c) => {
            if bin_fd >= 0 {
                unsafe { libc::close(bin_fd) };
            }
            c
        }
        Err(e) => {
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

    // Ensure all descendant processes in this group are terminated
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
        let mut dummy = 0;
        while libc::waitpid(-pgid, &mut dummy, libc::WNOHANG) > 0 {}
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

/// Fallback runner execution for non-Unix operating systems.
#[cfg(not(unix))]
pub fn execute_single_run(
    _executable: &Path,
    _input: &[u8],
    _limits: &RunnerLimits,
) -> SingleRunResult {
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
    let isolation_available = crate::isolation::is_linux_isolation_available();
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

    let (isolation_level, warnings) = if is_isolated {
        ("linux-namespaces-seccomp", Vec::new())
    } else {
        (
            "process-limits-only",
            vec!["not sandboxed: running with process resource limits only".to_string()],
        )
    };

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

    fn make_test_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bigolens_runner_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create test directory");
        dir
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
            allow_unsandboxed: true,
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
            ..Default::default()
        };
        let res = execute_single_run(&bin, b"", &limits);
        assert_ne!(res.status, BenchmarkRunStatus::Success);
        assert!(res.stderr.contains("PTRACE_BLOCKED"));

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
}
