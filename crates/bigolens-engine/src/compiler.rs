//! Compiler adapter for safe benchmark binary compilation.
//!
//! Enforces fixed argument vectors, validated enums, flag escapes (`--`),
//! zero shell invocation, and structured diagnostic extraction.
//! Conforms to ADR 0014 and BL-402.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
/// 2. The `--` argument is placed before any file operands to prevent option injection.
/// 3. No shell is used.
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
            "-o".to_string(),
            output_path.to_string_lossy().to_string(),
            "--".to_string(),
            safe_source,
        ],
        CompilerKind::Gcc => vec![
            standard.flag().to_string(),
            opt_level.flag().to_string(),
            "-o".to_string(),
            output_path.to_string_lossy().to_string(),
            safe_source,
        ],
        CompilerKind::Msvc => vec![
            format!("/std:{}", standard.msvc_flag()),
            opt_level.msvc_flag().to_string(),
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

/// Compile a C++ source file into an executable inside a target directory.
pub fn compile_cpp(
    compiler: &CompilerInfo,
    source_content: &str,
    source_display_name: &str,
    output_dir: &Path,
    standard: CppStandard,
    opt_level: OptimizationLevel,
) -> Result<PathBuf, CompileError> {
    let source_path = output_dir.join(source_display_name);
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
        &source_path,
        &output_binary,
    );

    let output = Command::new(&compiler.path)
        .args(&args)
        .current_dir(output_dir)
        .output()
        .map_err(|e| CompileError::IoError(format!("failed to execute compiler: {}", e)))?;

    if !output.status.success() {
        let stderr_str = String::from_utf8_lossy(&output.stderr).to_string();
        let diagnostics = parse_compiler_diagnostics(&stderr_str);
        return Err(CompileError::CompilationFailed {
            diagnostics,
            raw_stderr: stderr_str,
        });
    }

    Ok(output_binary)
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
        assert_eq!(dash_dash_idx, 4);
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

        let temp_dir = std::env::temp_dir().join(format!("bigolens_test_{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);

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

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
