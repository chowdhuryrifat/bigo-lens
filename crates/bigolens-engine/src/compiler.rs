//! Compiler adapter for safe benchmark binary compilation.
//!
//! Enforces fixed argument vectors, validated enums, flag escapes (`--`),
//! zero shell invocation, and structured diagnostic extraction.
//! Conforms to ADR 0014 and BL-402.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

pub const DEFAULT_COMPILE_TIMEOUT: Duration = Duration::from_secs(15);
pub const DEFAULT_MAX_COMPILER_OUTPUT_BYTES: usize = 512 * 1024; // 512 KiB

static PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[\s:;,"'\(])(/[^\s:;,"'\(\)]+|[a-zA-Z]:[/\\][^\s:;,"'\(\)]+)"#).unwrap()
});

/// Supported C++ standards from validated enums.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CppStandard {
    #[serde(rename = "c++11")]
    Cpp11,
    #[serde(rename = "c++14")]
    Cpp14,
    #[serde(rename = "c++17")]
    Cpp17,
    #[serde(rename = "c++20")]
    Cpp20,
    #[serde(rename = "c++23")]
    Cpp23,
}

impl CppStandard {
    pub fn flag(&self) -> &'static str {
        match self {
            CppStandard::Cpp11 => "-std=c++11",
            CppStandard::Cpp14 => "-std=c++14",
            CppStandard::Cpp17 => "-std=c++17",
            CppStandard::Cpp20 => "-std=c++20",
            CppStandard::Cpp23 => "-std=c++23",
        }
    }

    pub fn msvc_flag(&self) -> &'static str {
        match self {
            CppStandard::Cpp11 => "c++14", // MSVC lowest modern standard
            CppStandard::Cpp14 => "c++14",
            CppStandard::Cpp17 => "c++17",
            CppStandard::Cpp20 => "c++20",
            CppStandard::Cpp23 => "c++latest",
        }
    }
}

/// Validated compiler optimization levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationLevel {
    #[serde(rename = "O0")]
    O0,
    #[serde(rename = "O1")]
    O1,
    #[serde(rename = "O2")]
    O2,
    #[serde(rename = "O3")]
    O3,
}

impl OptimizationLevel {
    pub fn flag(&self) -> &'static str {
        match self {
            OptimizationLevel::O0 => "-O0",
            OptimizationLevel::O1 => "-O1",
            OptimizationLevel::O2 => "-O2",
            OptimizationLevel::O3 => "-O3",
        }
    }

    pub fn msvc_flag(&self) -> &'static str {
        match self {
            OptimizationLevel::O0 => "/Od",
            OptimizationLevel::O1 => "/O1",
            OptimizationLevel::O2 => "/O2",
            OptimizationLevel::O3 => "/Ox",
        }
    }
}

/// Compiler family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerKind {
    Clang,
    Gcc,
    Msvc,
}

/// Information about a discovered compiler executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilerInfo {
    pub kind: CompilerKind,
    pub path: PathBuf,
    pub version: String,
}

/// Structured compiler diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompileDiagnostic {
    pub file: String,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub severity: String,
    pub message: String,
}

/// Structured compilation error.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CompileError {
    #[error("compiler not found; searched for: {searched:?} in PATH={path_env:?}")]
    CompilerNotFound {
        searched: Vec<String>,
        path_env: Option<String>,
    },
    #[error("compilation timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },
    #[error("compilation output limit exceeded ({limit_bytes} bytes)")]
    OutputLimitExceeded { limit_bytes: usize },
    #[error("compilation failed with {} diagnostics: {raw_stderr}", diagnostics.len())]
    CompilationFailed {
        diagnostics: Vec<CompileDiagnostic>,
        raw_stderr: String,
    },
    #[error("I/O error during compilation: {0}")]
    IoError(String),
}

/// Build fixed argument vector for compilation.
///
/// Guaranteed properties:
/// 1. Only validated enum flags are used for options.
/// 2. For Clang, the `--` argument is placed before file operands to prevent option injection.
///    GCC does not accept `--`, so operands are sanitized to prevent leading-dash option injection.
/// 3. Resource limiting flags (-ftemplate-depth, -fconstexpr-depth, -fconstexpr-steps/-fconstexpr-ops-limit, -fmax-errors)
///    are enforced to mitigate compile-time recursion and allocation exhaustion.
/// 4. No shell is used.
pub fn build_compile_args(
    kind: CompilerKind,
    standard: CppStandard,
    opt_level: OptimizationLevel,
    source_path: &Path,
    output_path: &Path,
) -> Vec<String> {
    let safe_source = if source_path.is_absolute() {
        source_path.to_string_lossy().to_string()
    } else {
        let s = source_path.to_string_lossy();
        if s.starts_with('-') {
            format!("./{}", s)
        } else {
            s.to_string()
        }
    };

    match kind {
        CompilerKind::Clang => vec![
            standard.flag().to_string(),
            opt_level.flag().to_string(),
            "-ftemplate-depth=256".to_string(),
            "-fconstexpr-depth=256".to_string(),
            "-fconstexpr-steps=1048576".to_string(),
            "-fmax-errors=10".to_string(),
            "-o".to_string(),
            output_path.to_string_lossy().to_string(),
            "--".to_string(),
            safe_source,
        ],
        CompilerKind::Gcc => vec![
            standard.flag().to_string(),
            opt_level.flag().to_string(),
            "-ftemplate-depth=256".to_string(),
            "-fconstexpr-depth=256".to_string(),
            "-fconstexpr-ops-limit=10000000".to_string(),
            "-fmax-errors=10".to_string(),
            "-o".to_string(),
            output_path.to_string_lossy().to_string(),
            safe_source,
        ],
        CompilerKind::Msvc => vec![
            format!("/std:{}", standard.msvc_flag()),
            opt_level.msvc_flag().to_string(),
            "/constexpr:depth256".to_string(),
            format!("/Fe:{}", output_path.to_string_lossy()),
            safe_source,
        ],
    }
}

/// Discover a C++ compiler from PATH or custom environment variable.
pub fn discover_compiler(custom_path: Option<&Path>) -> Result<CompilerInfo, CompileError> {
    let mut searched = Vec::new();

    if let Some(p) = custom_path {
        searched.push(p.display().to_string());
        if p.exists() {
            if let Ok(info) = probe_compiler(p) {
                return Ok(info);
            }
        }
    }

    if let Ok(cxx) = env::var("CXX") {
        let p = PathBuf::from(&cxx);
        searched.push(cxx.clone());
        if p.exists() {
            if let Ok(info) = probe_compiler(&p) {
                return Ok(info);
            }
        }
    }

    // Default candidates
    let candidates = if cfg!(windows) {
        vec!["clang++.exe", "g++.exe", "cl.exe"]
    } else {
        vec!["clang++", "g++"]
    };

    for name in &candidates {
        searched.push(name.to_string());
        if let Some(path) = find_in_path(name) {
            if let Ok(info) = probe_compiler(&path) {
                return Ok(info);
            }
        }
    }

    Err(CompileError::CompilerNotFound {
        searched,
        path_env: env::var("PATH").ok(),
    })
}

fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let full_path = dir.join(binary_name);
        if full_path.is_file() {
            return Some(full_path);
        }
    }
    None
}

fn probe_compiler(path: &Path) -> Result<CompilerInfo, ()> {
    let output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|_| ())?;

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    let stderr_str = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout_str, stderr_str);

    let kind = if combined.contains("clang") {
        CompilerKind::Clang
    } else if combined.contains("Free Software Foundation") || combined.contains("g++") {
        CompilerKind::Gcc
    } else if combined.contains("Microsoft")
        || path.file_name().is_some_and(|f| f == "cl.exe" || f == "cl")
    {
        CompilerKind::Msvc
    } else {
        CompilerKind::Clang // fallback
    };

    let first_line = combined
        .lines()
        .next()
        .unwrap_or("unknown")
        .trim()
        .to_string();

    Ok(CompilerInfo {
        kind,
        path: path.to_path_buf(),
        version: first_line,
    })
}

/// Execution result of a compiler process run with limits.
pub struct RunCompilerOutput {
    pub status: std::process::ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Redact absolute filesystem paths outside scratch directory from diagnostics text.
///
/// Residual risk: An attacker might infer the existence or non-existence of files by examining
/// compiler error types (e.g. "No such file" vs "Permission denied"), but actual host filesystem
/// paths are redacted from the response.
pub fn redact_paths_in_text(text: &str, scratch_dir: &str) -> String {
    let scratch_norm = scratch_dir.replace('\\', "/");
    PATH_REGEX
        .replace_all(text, |caps: &regex::Captures| {
            let full_match = caps.get(0).unwrap().as_str();
            let path_part = caps.get(1).unwrap().as_str();
            let path_norm = path_part.replace('\\', "/");
            let prefix_len = full_match.len() - path_part.len();
            let prefix = &full_match[..prefix_len];

            if path_norm.starts_with(&scratch_norm) {
                full_match.to_string()
            } else {
                format!("{}[redacted_path]", prefix)
            }
        })
        .to_string()
}

/// Redact diagnostics file and message fields so that no host paths outside output_dir leak.
pub fn redact_diagnostics(
    mut diagnostics: Vec<CompileDiagnostic>,
    raw_stderr: &str,
    output_dir: &Path,
    source_display_name: &str,
) -> (Vec<CompileDiagnostic>, String) {
    let output_dir_str = output_dir.to_string_lossy();
    let safe_source_name = "solution.cpp";

    for diag in &mut diagnostics {
        if diag.file == safe_source_name
            || diag.file == format!("./{}", safe_source_name)
            || diag.file == output_dir.join(safe_source_name).to_string_lossy()
            || diag.file.ends_with(&format!("/{}", safe_source_name))
            || diag.file.ends_with(&format!("\\{}", safe_source_name))
        {
            diag.file = source_display_name.to_string();
        } else if Path::new(&diag.file).is_absolute()
            && !diag
                .file
                .replace('\\', "/")
                .starts_with(&output_dir_str.replace('\\', "/"))
        {
            diag.file = "[redacted_path]".to_string();
        }

        diag.message = redact_paths_in_text(&diag.message, output_dir_str.as_ref());
    }

    let redacted_stderr = redact_paths_in_text(raw_stderr, output_dir_str.as_ref());
    (diagnostics, redacted_stderr)
}

/// Run compiler process with a wall-clock timeout and output size cap.
///
/// On Unix, sets up a separate process group so timeout kills the entire compiler tree
/// (including any child driver, assembler, or linker processes).
pub fn run_compiler_with_timeout(
    cmd: &mut Command,
    timeout: Duration,
    max_output_bytes: usize,
) -> Result<RunCompilerOutput, CompileError> {
    use std::io::Read;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Instant;

    let mut child = cmd
        .spawn()
        .map_err(|e| CompileError::IoError(format!("failed to spawn compiler: {}", e)))?;

    #[cfg(unix)]
    let pgid = child.id() as libc::pid_t;

    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let exceeded_limit = Arc::new(AtomicBool::new(false));

    let stdout_exceeded = Arc::clone(&exceeded_limit);
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stream) = stdout_pipe {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if buf.len() + n > max_output_bytes {
                    stdout_exceeded.store(true, Ordering::Relaxed);
                    buf.extend_from_slice(&chunk[..max_output_bytes.saturating_sub(buf.len())]);
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        buf
    });

    let stderr_exceeded = Arc::clone(&exceeded_limit);
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut stream) = stderr_pipe {
            let mut chunk = [0u8; 4096];
            while let Ok(n) = stream.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if buf.len() + n > max_output_bytes {
                    stderr_exceeded.store(true, Ordering::Relaxed);
                    buf.extend_from_slice(&chunk[..max_output_bytes.saturating_sub(buf.len())]);
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
        }
        buf
    });

    let start = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout {
                    timed_out = true;
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                    #[cfg(not(unix))]
                    let _ = child.kill();
                    break child.wait().unwrap_or_else(|_| {
                        #[cfg(unix)]
                        use std::os::unix::process::ExitStatusExt;
                        #[cfg(unix)]
                        return std::process::ExitStatus::from_raw(9);
                        #[cfg(windows)]
                        use std::os::windows::process::ExitStatusExt;
                        #[cfg(windows)]
                        return std::process::ExitStatus::from_raw(1);
                    });
                }
                if exceeded_limit.load(Ordering::Relaxed) {
                    #[cfg(unix)]
                    unsafe {
                        libc::kill(-pgid, libc::SIGKILL);
                    }
                    #[cfg(not(unix))]
                    let _ = child.kill();
                    break child.wait().unwrap_or_else(|_| {
                        #[cfg(unix)]
                        use std::os::unix::process::ExitStatusExt;
                        #[cfg(unix)]
                        return std::process::ExitStatus::from_raw(9);
                        #[cfg(windows)]
                        use std::os::windows::process::ExitStatusExt;
                        #[cfg(windows)]
                        return std::process::ExitStatus::from_raw(1);
                    });
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-pgid, libc::SIGKILL);
                }
                #[cfg(not(unix))]
                let _ = child.kill();
                return Err(CompileError::IoError(format!(
                    "error waiting for compiler: {}",
                    e
                )));
            }
        }
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    if timed_out {
        return Err(CompileError::Timeout {
            timeout_secs: timeout.as_secs(),
        });
    }

    if exceeded_limit.load(Ordering::Relaxed) {
        return Err(CompileError::OutputLimitExceeded {
            limit_bytes: max_output_bytes,
        });
    }

    Ok(RunCompilerOutput {
        status,
        stdout,
        stderr,
    })
}

/// Resource limits for compiler invocation.
#[derive(Debug, Clone, Copy)]
pub struct CompileLimits {
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl Default for CompileLimits {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_COMPILE_TIMEOUT,
            max_output_bytes: DEFAULT_MAX_COMPILER_OUTPUT_BYTES,
        }
    }
}

/// Compile a C++ source file into an executable inside a target directory with specified limits.
pub fn compile_cpp_with_limits(
    compiler: &CompilerInfo,
    source_content: &str,
    source_display_name: &str,
    output_dir: &Path,
    standard: CppStandard,
    opt_level: OptimizationLevel,
    limits: CompileLimits,
) -> Result<PathBuf, CompileError> {
    const SAFE_SOURCE_NAME: &str = "solution.cpp";
    let source_path = output_dir.join(SAFE_SOURCE_NAME);
    fs::write(&source_path, source_content)
        .map_err(|e| CompileError::IoError(format!("failed to write source file: {}", e)))?;

    let binary_name = if cfg!(windows) {
        "benchmark_binary.exe"
    } else {
        "benchmark_binary"
    };
    let output_binary = output_dir.join(binary_name);

    let args = build_compile_args(
        compiler.kind,
        standard,
        opt_level,
        Path::new(SAFE_SOURCE_NAME),
        &output_binary,
    );

    let mut cmd = Command::new(&compiler.path);
    cmd.args(&args)
        .current_dir(output_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Cleared, minimal environment: explicit PATH only (plus Windows core variables if on Windows)
    cmd.env_clear();
    if let Some(path) = env::var_os("PATH") {
        cmd.env("PATH", path);
    }
    #[cfg(windows)]
    {
        if let Some(sysroot) = env::var_os("SystemRoot") {
            cmd.env("SystemRoot", sysroot);
        }
        if let Some(tmp) = env::var_os("TMP") {
            cmd.env("TMP", tmp);
        }
        if let Some(temp) = env::var_os("TEMP") {
            cmd.env("TEMP", temp);
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }

    let output = run_compiler_with_timeout(&mut cmd, limits.timeout, limits.max_output_bytes)?;

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        let diagnostics = parse_compiler_diagnostics(&stderr_str);
        let (redacted_diags, redacted_stderr) =
            redact_diagnostics(diagnostics, &stderr_str, output_dir, source_display_name);

        return Err(CompileError::CompilationFailed {
            diagnostics: redacted_diags,
            raw_stderr: redacted_stderr,
        });
    }

    Ok(output_binary)
}

/// Compile a C++ source file into an executable inside a target directory using default limits.
///
/// The source is written to a fixed, safe file (`solution.cpp`) inside `output_dir`
/// so that `source_display_name` can never cause path traversal, file overwrites,
/// or directory creation failures. Compiler diagnostics that reference `solution.cpp`
/// are mapped back to `source_display_name`.
pub fn compile_cpp(
    compiler: &CompilerInfo,
    source_content: &str,
    source_display_name: &str,
    output_dir: &Path,
    standard: CppStandard,
    opt_level: OptimizationLevel,
) -> Result<PathBuf, CompileError> {
    compile_cpp_with_limits(
        compiler,
        source_content,
        source_display_name,
        output_dir,
        standard,
        opt_level,
        CompileLimits::default(),
    )
}

/// Parse GCC and Clang stderr diagnostics.
pub fn parse_compiler_diagnostics(stderr: &str) -> Vec<CompileDiagnostic> {
    let mut diagnostics = Vec::new();

    for line in stderr.lines() {
        // e.g. "solution.cpp:5:10: error: expected ';' before '}'"
        let parts: Vec<&str> = line.splitn(5, ':').collect();
        if parts.len() >= 4 {
            let file = parts[0].trim().to_string();
            let line_num = parts[1].trim().parse::<u32>().ok();
            let col_num = parts[2].trim().parse::<u32>().ok();
            let sev_and_msg = parts[3..].join(":");
            let sev_trimmed = sev_and_msg.trim();

            let (severity, message) = if let Some(stripped) = sev_trimmed.strip_prefix("error:") {
                ("error".to_string(), stripped.trim().to_string())
            } else if let Some(stripped) = sev_trimmed.strip_prefix("warning:") {
                ("warning".to_string(), stripped.trim().to_string())
            } else if let Some(stripped) = sev_trimmed.strip_prefix("fatal error:") {
                ("error".to_string(), stripped.trim().to_string())
            } else {
                ("info".to_string(), sev_trimmed.to_string())
            };

            if line_num.is_some() {
                diagnostics.push(CompileDiagnostic {
                    file,
                    line: line_num,
                    column: col_num,
                    severity,
                    message,
                });
            }
        }
    }

    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_compile_args_has_fixed_structure() {
        let src = Path::new("/tmp/test.cpp");
        let out = Path::new("/tmp/test.out");
        let args = build_compile_args(
            CompilerKind::Clang,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
            src,
            out,
        );

        assert_eq!(
            args,
            vec![
                "-std=c++17",
                "-O2",
                "-ftemplate-depth=256",
                "-fconstexpr-depth=256",
                "-fconstexpr-steps=1048576",
                "-fmax-errors=10",
                "-o",
                "/tmp/test.out",
                "--",
                "/tmp/test.cpp"
            ]
        );
    }

    #[test]
    fn test_build_compile_args_operands_preceded_by_double_dash() {
        let flag_named_file = Path::new("/tmp/-fplugin=malicious.so");
        let out = Path::new("/tmp/out");
        let args = build_compile_args(
            CompilerKind::Clang,
            CppStandard::Cpp20,
            OptimizationLevel::O3,
            flag_named_file,
            out,
        );

        // For Clang, verify that "--" is located immediately prior to the source file operand
        let dash_dash_idx = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(dash_dash_idx, args.len() - 2);
        assert_eq!(args[dash_dash_idx + 1], "/tmp/-fplugin=malicious.so");

        // For GCC, relative paths starting with '-' are prefixed with './'
        let rel_flag_file = Path::new("-fplugin=malicious.so");
        let gcc_args = build_compile_args(
            CompilerKind::Gcc,
            CppStandard::Cpp20,
            OptimizationLevel::O3,
            rel_flag_file,
            out,
        );
        assert_eq!(gcc_args.last().unwrap(), "./-fplugin=malicious.so");
    }

    #[test]
    fn test_build_compile_args_all_compiler_kinds() {
        let src = Path::new("solution.cpp");
        let out = Path::new("bench_bin");

        // Clang
        let clang_args = build_compile_args(
            CompilerKind::Clang,
            CppStandard::Cpp20,
            OptimizationLevel::O3,
            src,
            out,
        );
        assert!(clang_args.contains(&"-std=c++20".to_string()));
        assert!(clang_args.contains(&"-O3".to_string()));
        assert!(clang_args.contains(&"-ftemplate-depth=256".to_string()));
        assert!(clang_args.contains(&"-fconstexpr-steps=1048576".to_string()));
        assert!(clang_args.contains(&"-fmax-errors=10".to_string()));
        let dash_dash = clang_args.iter().position(|a| a == "--").unwrap();
        assert_eq!(clang_args[dash_dash + 1], "solution.cpp");

        // GCC
        let gcc_args = build_compile_args(
            CompilerKind::Gcc,
            CppStandard::Cpp20,
            OptimizationLevel::O3,
            src,
            out,
        );
        assert!(gcc_args.contains(&"-std=c++20".to_string()));
        assert!(gcc_args.contains(&"-O3".to_string()));
        assert!(gcc_args.contains(&"-ftemplate-depth=256".to_string()));
        assert!(gcc_args.contains(&"-fconstexpr-ops-limit=10000000".to_string()));
        assert!(gcc_args.contains(&"-fmax-errors=10".to_string()));
        assert!(!gcc_args.contains(&"--".to_string()));
        assert_eq!(gcc_args.last().unwrap(), "solution.cpp");

        // MSVC
        let msvc_args = build_compile_args(
            CompilerKind::Msvc,
            CppStandard::Cpp20,
            OptimizationLevel::O3,
            src,
            out,
        );
        assert!(msvc_args.contains(&"/std:c++20".to_string()));
        assert!(msvc_args.contains(&"/Ox".to_string()));
        assert!(msvc_args.contains(&"/constexpr:depth256".to_string()));
        assert_eq!(msvc_args.last().unwrap(), "solution.cpp");
    }

    #[test]
    fn test_path_redaction_in_diagnostics() {
        let scratch_dir = "/tmp/bigolens_scratch_12345";
        let raw_stderr = "In file included from /tmp/bigolens_scratch_12345/solution.cpp:1:\n/etc/shadow:1:1: error: stray '\\x1f'\nsolution.cpp:2:3: fatal error: /home/secret/passwords.txt: No such file\n";
        let diags = vec![
            CompileDiagnostic {
                file: "/etc/shadow".to_string(),
                line: Some(1),
                column: Some(1),
                severity: "error".to_string(),
                message: "stray '\\x1f'".to_string(),
            },
            CompileDiagnostic {
                file: "solution.cpp".to_string(),
                line: Some(2),
                column: Some(3),
                severity: "error".to_string(),
                message: "fatal error: /home/secret/passwords.txt: No such file".to_string(),
            },
        ];

        let (redacted_diags, redacted_stderr) =
            redact_diagnostics(diags, raw_stderr, Path::new(scratch_dir), "my_display.cpp");

        assert_eq!(redacted_diags[0].file, "[redacted_path]");
        assert_eq!(redacted_diags[1].file, "my_display.cpp");
        assert!(redacted_diags[1].message.contains("[redacted_path]"));
        assert!(!redacted_diags[1]
            .message
            .contains("/home/secret/passwords.txt"));
        assert!(!redacted_stderr.contains("/etc/shadow"));
        assert!(!redacted_stderr.contains("/home/secret/passwords.txt"));
        assert!(redacted_stderr.contains("/tmp/bigolens_scratch_12345/solution.cpp"));
    }

    #[test]
    fn test_compile_timeout_and_output_limit() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return,
        };

        let temp_dir_obj = tempfile::tempdir().unwrap();

        // Test timeout with 1ns limit
        let code = "int main() { return 0; }\n";
        let res = compile_cpp_with_limits(
            &compiler,
            code,
            "main.cpp",
            temp_dir_obj.path(),
            CppStandard::Cpp17,
            OptimizationLevel::O2,
            CompileLimits {
                timeout: Duration::from_nanos(1), // Exceeds immediately
                max_output_bytes: DEFAULT_MAX_COMPILER_OUTPUT_BYTES,
            },
        );
        assert!(res.is_err());
        assert!(matches!(res.unwrap_err(), CompileError::Timeout { .. }));

        // Test output limit with 1 byte limit
        let res = compile_cpp_with_limits(
            &compiler,
            "int main() { syntax error }\n",
            "main.cpp",
            temp_dir_obj.path(),
            CppStandard::Cpp17,
            OptimizationLevel::O2,
            CompileLimits {
                timeout: Duration::from_secs(10),
                max_output_bytes: 1, // 1 byte limit
            },
        );
        assert!(res.is_err());
        assert!(matches!(
            res.unwrap_err(),
            CompileError::OutputLimitExceeded { .. }
        ));
    }

    #[test]
    fn test_missing_compiler_returns_structured_error() {
        // Probe a nonexistent compiler path
        let nonexistent = Path::new("/nonexistent/bin/cxx");
        let res = probe_compiler(nonexistent);
        assert!(res.is_err());
    }

    #[test]
    fn test_parse_compiler_diagnostics() {
        let stderr =
            "solution.cpp:4:9: error: 'x' was not declared in this scope\n    x = 10;\n    ^\n";
        let diags = parse_compiler_diagnostics(stderr);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].file, "solution.cpp");
        assert_eq!(diags[0].line, Some(4));
        assert_eq!(diags[0].column, Some(9));
        assert_eq!(diags[0].severity, "error");
        assert_eq!(diags[0].message, "'x' was not declared in this scope");
    }

    #[test]
    fn test_compile_success_and_error_with_system_compiler() {
        let compiler = match discover_compiler(None) {
            Ok(c) => c,
            Err(_) => return, // Skip if no compiler on system
        };

        let temp_dir_obj = tempfile::tempdir().unwrap();
        let temp_dir = temp_dir_obj.path().to_path_buf();

        // 1. Successful compilation
        let valid_code = "int main() { return 0; }\n";
        let bin_res = compile_cpp(
            &compiler,
            valid_code,
            "main.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(
            bin_res.is_ok(),
            "compile should succeed: {:?}",
            bin_res.err()
        );
        let bin = bin_res.unwrap();
        assert!(bin.exists());

        // 2. Compilation failure with structured diagnostics
        let invalid_code = "int main() { syntax error }\n";
        let err_res = compile_cpp(
            &compiler,
            invalid_code,
            "broken.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(err_res.is_err());
        match err_res.unwrap_err() {
            CompileError::CompilationFailed { diagnostics, .. } => {
                assert!(!diagnostics.is_empty());
                assert_eq!(diagnostics[0].severity, "error");
            }
            other => panic!("expected CompilationFailed, got {:?}", other),
        }

        // 3. Source file named like a flag
        let flag_named_code = "int main() { return 0; }\n";
        let flag_res = compile_cpp(
            &compiler,
            flag_named_code,
            "-fplugin=fake.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(
            flag_res.is_ok(),
            "should safely compile file named like a flag: {:?}",
            flag_res.err()
        );

        // 4. Nested path should compile safely and not fail because parent directories don't exist
        let nested_res = compile_cpp(
            &compiler,
            valid_code,
            "nested/sub/dir/solution.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(
            nested_res.is_ok(),
            "should succeed with nested path, but got: {:?}",
            nested_res.err()
        );
        assert!(!temp_dir.join("nested").exists());

        // 5. Absolute path does NOT write outside the temp directory
        let outside_dir = temp_dir.join("outside_scratch");
        fs::create_dir_all(&outside_dir).unwrap();
        let outside_file = outside_dir.join("victim.cpp");
        assert!(!outside_file.exists());

        let abs_res = compile_cpp(
            &compiler,
            valid_code,
            outside_file.to_str().unwrap(),
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(
            abs_res.is_ok(),
            "compile should succeed: {:?}",
            abs_res.err()
        );
        assert!(
            !outside_file.exists(),
            "absolute path must NOT be written to"
        );

        // 6. Path traversal (../../x.cpp) does NOT escape the temp directory
        let escape_target = temp_dir.parent().unwrap().join("escaped_victim.cpp");
        if escape_target.exists() {
            let _ = fs::remove_file(&escape_target);
        }

        let traversal_res = compile_cpp(
            &compiler,
            valid_code,
            "../../escaped_victim.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        );
        assert!(
            traversal_res.is_ok(),
            "compile should succeed: {:?}",
            traversal_res.err()
        );
        assert!(
            !escape_target.exists(),
            "traversal path must NOT be written outside temp_dir"
        );

        // 7. Compiler diagnostics are mapped back to source_display_name
        let broken_display = "user/project/my_code.cpp";
        let diag_err = compile_cpp(
            &compiler,
            invalid_code,
            broken_display,
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        )
        .unwrap_err();
        match diag_err {
            CompileError::CompilationFailed { diagnostics, .. } => {
                assert!(!diagnostics.is_empty());
                assert_eq!(diagnostics[0].file, broken_display);
            }
            other => panic!("expected CompilationFailed, got {:?}", other),
        }

        // 8. Compiler diagnostics redact external absolute paths
        let evil_include_code = "#include \"/etc/shadow\"\nint main() { return 0; }\n";
        let redact_err = compile_cpp(
            &compiler,
            evil_include_code,
            "main.cpp",
            &temp_dir,
            CppStandard::Cpp17,
            OptimizationLevel::O2,
        )
        .unwrap_err();
        match redact_err {
            CompileError::CompilationFailed {
                diagnostics,
                raw_stderr,
            } => {
                assert!(!diagnostics.is_empty());
                assert!(!raw_stderr.contains("/etc/shadow"));
                for d in &diagnostics {
                    assert!(!d.file.contains("/etc/shadow"));
                    assert!(!d.message.contains("/etc/shadow"));
                }
            }
            other => panic!("expected CompilationFailed, got {:?}", other),
        }
    }
}
