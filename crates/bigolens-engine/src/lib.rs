//! BigO Lens standalone engine library.
//!
//! Exposes compiler adapters, isolation primitives, benchmark runners,
//! static analysis routines, and protocol dispatchers.

pub mod compiler;
pub mod differential;
pub mod isolation;
pub mod runner;

use bigolens_core::api::{
    Request, ENGINE_NAME, ENGINE_VERSION, MAX_REQUEST_BYTES, PROTOCOL_VERSION,
};
use bigolens_core::models::{
    AnalyzeResults, BoundStatus, EngineMetadata, MemoryResult, ProtocolError, Response, Summary,
};
use std::fs;
use std::path::PathBuf;

/// Process a JSON request string and return the single-line JSON response.
pub fn handle_request(text: &str) -> String {
    if text.len() > MAX_REQUEST_BYTES {
        let resp = error_response(
            None,
            "request_too_large",
            "request size exceeds the 4 MiB limit",
            None,
        );
        return serde_json::to_string(&resp).unwrap();
    }

    let parsed: serde_json::Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            let resp = error_response(
                None,
                "malformed_json",
                &format!("JSON parse error: {}", e),
                None,
            );
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let request_id = parsed
        .get("request_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let req: Request = match serde_json::from_value(parsed) {
        Ok(r) => r,
        Err(e) => {
            let resp = error_response(
                request_id,
                "invalid_request",
                &format!("schema validation error: {}", e),
                None,
            );
            return serde_json::to_string(&resp).unwrap();
        }
    };

    let resp = dispatch_request(req);
    serde_json::to_string(&resp).unwrap()
}

pub fn dispatch_request(req: Request) -> Response {
    match bigolens_core::api::is_supported_schema_version(&req.schema_version, PROTOCOL_VERSION) {
        Ok(true) => {}
        Ok(false) => {
            return error_response(
                Some(req.request_id),
                "unsupported_schema_version",
                &format!(
                    "unsupported schema version '{}', expected '{}'",
                    req.schema_version, PROTOCOL_VERSION
                ),
                Some("/schema_version".to_string()),
            );
        }
        Err(_) => {
            return error_response(
                Some(req.request_id),
                "invalid_request",
                &format!(
                    "invalid schema version format '{}', expected MAJOR.MINOR",
                    req.schema_version
                ),
                Some("/schema_version".to_string()),
            );
        }
    }

    match req.action.as_str() {
        "capabilities" => capabilities_response(req.request_id),
        "analyze" => analyze_action(req),
        "benchmark" => benchmark_action(req),
        _ => error_response(
            Some(req.request_id),
            "not_implemented",
            &format!("action '{}' is not implemented", req.action),
            Some("/action".to_string()),
        ),
    }
}

pub fn capabilities_response(request_id: String) -> Response {
    let caps = serde_json::json!({
        "actions": ["capabilities", "analyze", "benchmark"],
        "features": {
            "benchmark": "supported",
            "constraint_workload": "partial",
            "interprocedural_analysis": "partial",
            "loop_analysis": "partial",
            "range_for_loops": "partial",
            "recurrence_extraction": "partial",
            "recursion_analysis": "partial",
            "static_memory_estimate": "partial",
            "stl_cost_model": "partial",
            "time_feasibility": "supported",
            "while_countdown_loops": "partial"
        },
        "stl_cost_model": [
            {
                "amortized": false,
                "average": "1",
                "container": "std::vector",
                "note": "[vector.access]: constant time",
                "operation": "operator[]",
                "worst": "1"
            },
            {
                "amortized": true,
                "average": "1",
                "container": "std::vector",
                "note": "[vector.modifiers]: amortized constant; a single call may reallocate and copy n elements",
                "operation": "push_back",
                "worst": "n"
            }
        ]
    });

    Response {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id: Some(request_id),
        status: "ok".to_string(),
        engine: engine_metadata(),
        results: Some(caps),
        diagnostics: Vec::new(),
        errors: Vec::new(),
    }
}

pub fn run_static_analysis(
    source: &bigolens_core::api::SourcePayload,
    constraints: &bigolens_core::constraints::ConstraintSet,
) -> (Vec<bigolens_core::models::Finding>, MemoryResult, Summary) {
    let (findings, max_workload) = bigolens_syntax::analyze_source(
        &source.content,
        &source.language,
        &source.path,
        constraints,
    );

    let mut dynamic_bytes = 0.0;
    let mut dynamic_status = BoundStatus::Proven;
    for f in &findings {
        if let Some(ref sp) = f.space {
            if let Some(ub) = sp.workload.upper_bound {
                dynamic_bytes += ub;
            }
            dynamic_status = dynamic_status.weakest(sp.workload.status);
        }
    }
    let estimated_min = if dynamic_bytes > 0.0 {
        Some(dynamic_bytes)
    } else {
        Some(0.0)
    };

    let memory = MemoryResult {
        static_storage_bytes: 0,
        static_storage_saturated: false,
        largest_local_array_bytes: 0,
        estimated_minimum_bytes: estimated_min,
        estimated_minimum_status: dynamic_status,
        assumptions: vec![
            "process baseline, stack frames, standard-library internals and unmodelled heap allocations are not included; figures are lower bounds, not peak usage".to_string(),
        ],
        items: Vec::new(),
    };

    let mut dominant_finding_id: Option<String> = None;
    if let Some(ref mw) = max_workload {
        if let Some(ub) = mw.upper_bound {
            for f in &findings {
                if let Some(ref t) = f.time {
                    if t.workload.upper_bound == Some(ub) {
                        dominant_finding_id = Some(f.id.clone());
                        break;
                    }
                }
            }
        }
    }
    if dominant_finding_id.is_none() && !findings.is_empty() {
        dominant_finding_id = Some(findings[0].id.clone());
    }

    let summary = Summary {
        finding_count: findings.len(),
        max_time_workload: max_workload.as_ref().and_then(|w| w.upper_bound),
        dominant_finding_id,
        coverage: "partial".to_string(),
    };

    (findings, memory, summary)
}

pub fn analyze_action(req: Request) -> Response {
    let source = match req.source {
        Some(s) => s,
        None => {
            return error_response(
                Some(req.request_id),
                "analyze_requires_source",
                "source payload is required for analyze",
                Some("/source".to_string()),
            );
        }
    };

    let constraints = req.constraints.unwrap_or_default();
    let (findings, memory, summary) = run_static_analysis(&source, &constraints);

    let feasibility = bigolens_core::feasibility::assess_feasibility(None, &memory, &constraints);

    let results = AnalyzeResults {
        findings,
        memory,
        feasibility,
        summary,
    };

    Response {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id: Some(req.request_id),
        status: "ok".to_string(),
        engine: engine_metadata(),
        results: Some(serde_json::to_value(results).unwrap()),
        diagnostics: Vec::new(),
        errors: Vec::new(),
    }
}

pub struct TempDirGuard(pub PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn benchmark_action(req: Request) -> Response {
    let source = match req.source {
        Some(s) => s,
        None => {
            return error_response(
                Some(req.request_id),
                "invalid_request",
                "source payload is required for benchmark",
                Some("/source".to_string()),
            );
        }
    };

    if source.language != "cpp" && source.language != "c++" {
        return error_response(
            Some(req.request_id),
            "not_implemented",
            &format!(
                "dynamic benchmarking for language '{}' is not yet supported",
                source.language
            ),
            Some("/source/language".to_string()),
        );
    }

    let compiler = match compiler::discover_compiler(None) {
        Ok(c) => c,
        Err(e) => {
            return error_response(
                Some(req.request_id),
                "frontend_unavailable",
                &format!("no C++ compiler discovered: {}", e),
                None,
            );
        }
    };

    let standard = match req.options.as_ref().and_then(|o| o.cpp_standard.as_deref()) {
        Some("c++11") => compiler::CppStandard::Cpp11,
        Some("c++14") => compiler::CppStandard::Cpp14,
        Some("c++20") => compiler::CppStandard::Cpp20,
        Some("c++23") => compiler::CppStandard::Cpp23,
        _ => compiler::CppStandard::Cpp17,
    };

    let temp_dir = std::env::temp_dir().join(format!(
        "bigolens_bench_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    if let Err(e) = fs::create_dir_all(&temp_dir) {
        return error_response(
            Some(req.request_id),
            "internal_error",
            &format!("failed to create temporary scratch directory: {}", e),
            None,
        );
    }
    let _guard = TempDirGuard(temp_dir.clone());

    let bin_path = match compiler::compile_cpp(
        &compiler,
        &source.content,
        &source.path,
        &temp_dir,
        standard,
        compiler::OptimizationLevel::O2,
    ) {
        Ok(p) => p,
        Err(compile_err) => {
            let protocol_diags = match compile_err {
                compiler::CompileError::CompilationFailed { diagnostics, .. } => diagnostics
                    .into_iter()
                    .map(|d| bigolens_core::models::Diagnostic {
                        severity: if d.severity.contains("error") {
                            bigolens_core::models::Severity::Error
                        } else if d.severity.contains("warning") {
                            bigolens_core::models::Severity::Warning
                        } else {
                            bigolens_core::models::Severity::Information
                        },
                        message: d.message,
                        location: d.line.map(|l| bigolens_core::models::SourceLocation {
                            file: source.path.clone(),
                            line: l,
                            column: d.column.unwrap_or(1),
                            end_line: None,
                            end_column: None,
                        }),
                    })
                    .collect(),
                _ => Vec::new(),
            };

            return Response {
                schema_version: PROTOCOL_VERSION.to_string(),
                request_id: Some(req.request_id),
                status: "error".to_string(),
                engine: engine_metadata(),
                results: None,
                diagnostics: protocol_diags,
                errors: vec![ProtocolError {
                    code: "source_parse_error".to_string(),
                    message: "benchmark source compilation failed".to_string(),
                    path: Some("/source".to_string()),
                }],
            };
        }
    };

    let constraints = req.constraints.unwrap_or_default();
    let (findings, memory, summary) = run_static_analysis(&source, &constraints);
    let config = req.benchmark_config.unwrap_or_default();

    // Determine target size and primary variable name
    let mut primary_var = "n".to_string();
    let mut target_n = 200_000.0;
    for (name, bound) in &constraints.variables {
        if let Some(mx) = bound.max {
            primary_var = name.clone();
            target_n = mx;
            break;
        }
    }

    let sizes = if !config.sizes.is_empty() {
        config.sizes.clone()
    } else {
        let max_scale = (target_n.min(50_000.0) as usize).max(400);
        let n1 = (max_scale / 8).max(50);
        let n2 = n1 * 2;
        let n3 = n2 * 2;
        let n4 = n3 * 2;
        vec![n1, n2, n3, n4]
    };

    let mem_bytes = (constraints.memory_limit_mb.unwrap_or(256.0) * 1024.0 * 1024.0) as u64;
    let limits = runner::RunnerLimits {
        wall_timeout: std::time::Duration::from_millis(config.per_run_timeout_ms),
        cpu_timeout: std::time::Duration::from_millis(config.per_run_timeout_ms),
        max_memory_bytes: mem_bytes,
        max_output_bytes: 10 * 1024 * 1024,
        max_nproc: None,
        max_nofile: 256,
        isolate: true,
        allow_unsandboxed: config.allow_unsandboxed,
    };

    let mut samples_json = Vec::new();
    let mut measurements = Vec::new();
    let mut max_measured_n = 0.0;
    let mut run_warnings = Vec::new();
    let mut isolation_level_applied = "process-limits-only";
    let mut isolated_applied = false;

    let gen_config = bigolens_core::generator::GeneratorConfig {
        seed: config.seed,
        byte_limit: 16 * 1024 * 1024,
    };

    for &size in &sizes {
        let mut sizes_map = std::collections::HashMap::new();
        sizes_map.insert(primary_var.clone(), size as u64);

        let format = bigolens_core::generator::InputFormat::Array {
            size_variable: primary_var.clone(),
            min_val: 1,
            max_val: 1_000_000,
        };

        let input_str = bigolens_core::generator::generate_input(
            &format,
            &sizes_map,
            &constraints,
            &gen_config,
        )
        .unwrap_or_else(|_| format!("{}\n", size));
        let input_bytes = input_str.as_bytes();

        let sample = runner::run_benchmark_series(
            &bin_path,
            input_bytes,
            &limits,
            config.warmup,
            config.repetitions,
        );

        isolation_level_applied = sample.isolation_level;
        isolated_applied = sample.isolated;
        for w in sample.warnings {
            if !run_warnings.contains(&w) {
                run_warnings.push(w);
            }
        }

        match sample.status {
            runner::BenchmarkRunStatus::Success => {
                let med = sample
                    .wall_time_summary
                    .as_ref()
                    .map(|s| s.median)
                    .unwrap_or(0.0);
                let mad = sample
                    .wall_time_summary
                    .as_ref()
                    .map(|s| s.mad)
                    .unwrap_or(0.0);
                let mut size_map = serde_json::Map::new();
                size_map.insert(primary_var.clone(), serde_json::json!(size));

                samples_json.push(serde_json::json!({
                    "size": size_map,
                    "wall_time_median_ms": med,
                    "wall_time_mad_ms": mad,
                    "peak_rss_bytes": sample.peak_rss_bytes,
                    "raw_samples_ms": sample.raw_wall_times_ms,
                }));

                measurements.push(bigolens_core::growth_fit::Measurement::new(
                    size as f64,
                    med,
                ));
                if size as f64 > max_measured_n {
                    max_measured_n = size as f64;
                }
            }
            runner::BenchmarkRunStatus::Error(ref err) if err.contains("isolation_unavailable") => {
                return error_response(Some(req.request_id), "isolation_unavailable", err, None);
            }
            other => {
                run_warnings.push(format!("size {}: {:?}", size, other));
                break;
            }
        }
    }

    let fit_options = bigolens_core::growth_fit::GrowthFitOptions::default();
    let fit_result = bigolens_core::growth_fit::fit_growth(&measurements, &fit_options);

    let (empirical_fit_json, runtime_estimate) = match fit_result {
        Some(res) => {
            let selected_model = res.selected;
            let ambiguous = res.ambiguous;

            let best_fit = res.fits.first();
            let (model_val, r2, rss, slope, intercept) = match (selected_model, best_fit) {
                (Some(m), Some(f)) => (
                    serde_json::json!(m.as_str()),
                    f.r2,
                    f.rss,
                    f.slope,
                    f.intercept,
                ),
                (None, Some(f)) => (serde_json::Value::Null, f.r2, f.rss, f.slope, f.intercept),
                _ => (serde_json::Value::Null, 0.0, 0.0, 0.0, 0.0),
            };

            let emp_fit = serde_json::json!({
                "model": model_val,
                "r_squared": r2,
                "rss": rss,
                "slope": slope,
                "intercept": intercept,
                "ambiguous": ambiguous,
            });

            let mut target_map = std::collections::BTreeMap::new();
            target_map.insert(primary_var.clone(), target_n);

            let (min_ms, central_ms, max_ms) = if let Some(m) = selected_model {
                if let Some(f) = res.fits.iter().find(|fit| fit.model == m) {
                    let pred = bigolens_core::growth_fit::predict(f, target_n).max(0.0);
                    ((pred * 0.9).max(0.0), pred, pred * 1.1)
                } else {
                    (0.0, 0.0, 0.0)
                }
            } else if !res.fits.is_empty() {
                let preds: Vec<f64> = res
                    .fits
                    .iter()
                    .map(|f| bigolens_core::growth_fit::predict(f, target_n).max(0.0))
                    .collect();
                let min_p = preds.iter().copied().fold(f64::INFINITY, f64::min);
                let max_p = preds.iter().copied().fold(0.0, f64::max);
                let central_p = preds[0];
                (min_p, central_p, max_p)
            } else {
                (0.0, 0.0, 0.0)
            };

            let est = bigolens_core::feasibility::RuntimeEstimate {
                target_size: target_map,
                min_ms,
                central_ms,
                max_ms,
                extrapolated: target_n > max_measured_n,
            };

            (emp_fit, Some(est))
        }
        None => {
            run_warnings.push(
                "insufficient measurement points or non-positive growth to fit model".to_string(),
            );
            let emp_fit = serde_json::json!({
                "model": serde_json::Value::Null,
                "r_squared": 0.0,
                "rss": 0.0,
                "slope": 0.0,
                "intercept": 0.0,
                "ambiguous": true,
            });
            (emp_fit, None)
        }
    };

    let feasibility = bigolens_core::feasibility::assess_feasibility(
        runtime_estimate.as_ref(),
        &memory,
        &constraints,
    );

    let mut target_obj = serde_json::Map::new();
    target_obj.insert(primary_var, serde_json::json!(target_n));

    let extrapolated_runtime_json = if let Some(ref est) = runtime_estimate {
        serde_json::json!({
            "target_size": target_obj,
            "min_ms": est.min_ms,
            "central_ms": est.central_ms,
            "max_ms": est.max_ms,
            "extrapolated": est.extrapolated,
        })
    } else {
        serde_json::json!({
            "target_size": target_obj,
            "min_ms": 0.0,
            "central_ms": 0.0,
            "max_ms": 0.0,
            "extrapolated": false,
        })
    };

    Response {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id: Some(req.request_id),
        status: "ok".to_string(),
        engine: engine_metadata(),
        results: Some(serde_json::json!({
            "findings": findings,
            "memory": memory,
            "feasibility": feasibility,
            "summary": summary,
            "benchmark": {
                "isolated": isolated_applied,
                "isolation_level": isolation_level_applied,
                "samples": samples_json,
                "empirical_fit": empirical_fit_json,
                "extrapolated_runtime": extrapolated_runtime_json,
                "warnings": run_warnings,
            }
        })),
        diagnostics: Vec::new(),
        errors: Vec::new(),
    }
}

pub fn engine_metadata() -> EngineMetadata {
    EngineMetadata {
        name: ENGINE_NAME.to_string(),
        version: ENGINE_VERSION.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
        frontend: Some("bigolens-syntax".to_string()),
    }
}

pub fn error_response(
    request_id: Option<String>,
    code: &str,
    message: &str,
    path: Option<String>,
) -> Response {
    Response {
        schema_version: PROTOCOL_VERSION.to_string(),
        request_id,
        status: "error".to_string(),
        engine: engine_metadata(),
        results: None,
        diagnostics: Vec::new(),
        errors: vec![ProtocolError {
            code: code.to_string(),
            message: message.to_string(),
            path,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capabilities_action() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-cap-1",
            "action": "capabilities"
        });
        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["request_id"], "test-cap-1");
        assert_eq!(resp["schema_version"], "1.1");

        let results = &resp["results"];
        assert_eq!(results["features"]["benchmark"], "supported");
        assert!(results["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "analyze"));
        assert!(results["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "capabilities"));
        assert!(results["actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l == "benchmark"));
    }

    #[test]
    fn test_analyze_python_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-py-1",
            "action": "analyze",
            "source": {
                "path": "test.py",
                "language": "python",
                "content": "def process(n, m):\n    grid = [[0] * m for _ in range(n)]\n    for i in range(n):\n        for j in range(m):\n            grid[i][j] = i + j\n"
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1.0, "max": 1000.0 },
                    "m": { "min": 1.0, "max": 1000.0 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["request_id"], "test-py-1");

        let results = &resp["results"];
        let findings = results["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 2);

        // One dynamic allocation, one nested loop
        assert!(findings
            .iter()
            .any(|f| f["category"] == "dynamic_allocation"));
        assert!(findings.iter().any(|f| f["category"] == "loop_nest"));

        // Feasibility
        assert_eq!(results["feasibility"]["time"]["status"], "unknown");
        assert_eq!(results["feasibility"]["memory"]["status"], "likely_safe");

        // Summary
        assert_eq!(results["summary"]["finding_count"], 2);
        assert_eq!(results["summary"]["max_time_workload"], 1000000.0);
    }

    #[test]
    fn test_analyze_cpp_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-cpp-1",
            "action": "analyze",
            "source": {
                "path": "triangular.cpp",
                "language": "cpp",
                "content": "int main() {\n    int count = 0;\n    for (int i = 0; i < n; ++i) {\n        for (int j = 0; j < i; ++j) {\n            ++count;\n        }\n    }\n    return 0;\n}\n"
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1.0, "max": 2000.0 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let results = &resp["results"];
        let findings = results["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["pattern"], "triangular-loop");
        assert_eq!(findings[0]["time"]["asymptotic"], "O(n^2)");
        assert_eq!(results["summary"]["max_time_workload"], 4000000.0);
    }

    #[test]
    fn test_analyze_recursion_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-rec-1",
            "action": "analyze",
            "source": {
                "path": "sort.py",
                "language": "python",
                "content": "def merge_sort(n):\n    if n <= 1:\n        return 1\n    for i in range(n):\n        pass\n    return merge_sort(n // 2) + merge_sort(n // 2)\n"
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1.0, "max": 1024.0 }
                },
                "time_limit_seconds": 1.0
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let results = &resp["results"];
        let findings = results["findings"].as_array().unwrap();
        assert!(findings.iter().any(|f| f["category"] == "recursion"));
    }

    #[test]
    fn test_analyze_java_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-java-1",
            "action": "analyze",
            "source": {
                "path": "Solution.java",
                "language": "java",
                "content": "class Solution { public void solve(int n) { for (int i = 0; i < n; i++) {} } }"
            },
            "constraints": {
                "variables": { "n": { "min": 1.0, "max": 1000.0 } }
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "ok");
        let findings = resp["results"]["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["time"]["asymptotic"], "O(n)");
    }

    #[test]
    fn test_analyze_rust_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-rust-1",
            "action": "analyze",
            "source": {
                "path": "main.rs",
                "language": "rust",
                "content": "fn solve(n: usize) { for i in 0..n { for j in 0..i {} } }"
            },
            "constraints": {
                "variables": { "n": { "min": 1.0, "max": 100.0 } }
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "ok");
        let findings = resp["results"]["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["time"]["asymptotic"], "O(n^2)");
    }

    #[test]
    fn test_analyze_javascript_request() {
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "test-js-1",
            "action": "analyze",
            "source": {
                "path": "app.js",
                "language": "javascript",
                "content": "function compute(n) { const arr = new Array(n); for (let i = 0; i < n; i++) {} }"
            },
            "constraints": {
                "variables": { "n": { "min": 1.0, "max": 500.0 } }
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "ok");
        let findings = resp["results"]["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 2);
    }

    #[test]
    fn test_error_handling() {
        // 1. Malformed JSON
        let resp_str = handle_request("not json");
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "malformed_json");

        // 2. Unsupported schema version
        let req = serde_json::json!({
            "schema_version": "2.0",
            "request_id": "err-1",
            "action": "capabilities"
        });
        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "unsupported_schema_version");

        // 3. Analyze missing source
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "err-2",
            "action": "analyze"
        });
        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "analyze_requires_source");

        // 4. Unknown action
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "err-3",
            "action": "unknown_action"
        });
        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "not_implemented");
    }

    #[test]
    fn test_semver_compatibility() {
        // "1.0" accepted
        let req = serde_json::json!({
            "schema_version": "1.0",
            "request_id": "v-1-0",
            "action": "capabilities"
        });
        let resp: serde_json::Value =
            serde_json::from_str(&handle_request(&req.to_string())).unwrap();
        assert_eq!(resp["status"], "ok");

        // "1.1" accepted
        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "v-1-1",
            "action": "capabilities"
        });
        let resp: serde_json::Value =
            serde_json::from_str(&handle_request(&req.to_string())).unwrap();
        assert_eq!(resp["status"], "ok");

        // "1.2" rejected with unsupported_schema_version
        let req = serde_json::json!({
            "schema_version": "1.2",
            "request_id": "v-1-2",
            "action": "capabilities"
        });
        let resp: serde_json::Value =
            serde_json::from_str(&handle_request(&req.to_string())).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "unsupported_schema_version");

        // "2.0" rejected with unsupported_schema_version
        let req = serde_json::json!({
            "schema_version": "2.0",
            "request_id": "v-2-0",
            "action": "capabilities"
        });
        let resp: serde_json::Value =
            serde_json::from_str(&handle_request(&req.to_string())).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "unsupported_schema_version");

        // "invalid" rejected with invalid_request
        let req = serde_json::json!({
            "schema_version": "not_semver",
            "request_id": "v-bad",
            "action": "capabilities"
        });
        let resp: serde_json::Value =
            serde_json::from_str(&handle_request(&req.to_string())).unwrap();
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "invalid_request");
    }

    fn fixture_path(name: &str) -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let p = manifest_dir.join("../../tests/fixtures/cpp").join(name);
        if p.exists() {
            p
        } else {
            std::path::PathBuf::from("tests/fixtures/cpp").join(name)
        }
    }

    #[test]
    fn test_benchmark_action_linear_fixture() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let source = std::fs::read_to_string(fixture_path("01_linear_loop.cpp"))
            .expect("failed to read fixture 01");

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-01",
            "action": "benchmark",
            "source": {
                "path": "01_linear_loop.cpp",
                "language": "cpp",
                "content": source
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1, "max": 100000 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            },
            "benchmark_config": {
                "sizes": [5000, 10000, 20000, 40000],
                "repetitions": 3,
                "warmup": 1,
                "per_run_timeout_ms": 3000,
                "allow_unsandboxed": true
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["schema_version"], "1.1");
        let results = &resp["results"];

        let bench = &results["benchmark"];
        let samples = bench["samples"].as_array().unwrap();
        assert_eq!(samples.len(), 4);

        let fit = &bench["empirical_fit"];
        assert!(!fit["rss"].as_f64().unwrap().is_nan());

        let runtime = &bench["extrapolated_runtime"];
        assert_eq!(runtime["extrapolated"], true);

        // Time feasibility evaluated
        let time_feas = &results["feasibility"]["time"];
        assert_ne!(time_feas["status"], "unknown");
    }

    #[test]
    fn test_benchmark_action_triangular_fixture() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let source = std::fs::read_to_string(fixture_path("03_triangular_loop.cpp"))
            .expect("failed to read fixture 03");

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-03",
            "action": "benchmark",
            "source": {
                "path": "03_triangular_loop.cpp",
                "language": "cpp",
                "content": source
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1, "max": 10000 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            },
            "benchmark_config": {
                "sizes": [500, 1000, 2000, 4000],
                "repetitions": 3,
                "warmup": 1,
                "allow_unsandboxed": true
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let bench = &resp["results"]["benchmark"];
        assert_eq!(bench["samples"].as_array().unwrap().len(), 4);
        assert!(!bench["empirical_fit"]["rss"].as_f64().unwrap().is_nan());
    }

    #[test]
    fn test_benchmark_action_quadratic_fit() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let source = r#"
#include <iostream>
#include <vector>

int main() {
    int n;
    if (!(std::cin >> n)) return 0;
    std::vector<int> v(n, 1);
    long long acc = 0;
    for (int i = 0; i < n; ++i) {
        for (int j = 0; j < n; ++j) {
            acc += v[j];
        }
    }
    std::cout << acc << "\n";
    return 0;
}
"#;

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-quad",
            "action": "benchmark",
            "source": {
                "path": "quad.cpp",
                "language": "cpp",
                "content": source
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1, "max": 20000 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            },
            "benchmark_config": {
                "sizes": [1000, 2000, 4000, 8000],
                "repetitions": 3,
                "warmup": 1,
                "per_run_timeout_ms": 3000,
                "allow_unsandboxed": true
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let bench = &resp["results"]["benchmark"];
        let fit = &bench["empirical_fit"];
        if fit["ambiguous"] != true {
            assert_eq!(fit["model"], "quadratic");
        }
    }

    #[test]
    fn test_benchmark_action_sorting_fixture() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let source = std::fs::read_to_string(fixture_path("05_sorting.cpp"))
            .expect("failed to read fixture 05");

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-05",
            "action": "benchmark",
            "source": {
                "path": "05_sorting.cpp",
                "language": "cpp",
                "content": source
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1, "max": 100000 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            },
            "benchmark_config": {
                "sizes": [5000, 10000, 20000, 40000],
                "repetitions": 3,
                "warmup": 1,
                "per_run_timeout_ms": 3000,
                "allow_unsandboxed": true
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let bench = &resp["results"]["benchmark"];
        assert_eq!(bench["samples"].as_array().unwrap().len(), 4);
        assert!(!bench["empirical_fit"]["rss"].as_f64().unwrap().is_nan());
    }

    #[test]
    fn test_benchmark_action_ambiguous_fit() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let source = r#"
#include <iostream>
int main() {
    int n;
    if (std::cin >> n) {
        std::cout << 42 << "\n";
    }
    return 0;
}
"#;

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-ambiguous",
            "action": "benchmark",
            "source": {
                "path": "constant.cpp",
                "language": "cpp",
                "content": source
            },
            "constraints": {
                "variables": {
                    "n": { "min": 1, "max": 100000 }
                },
                "time_limit_seconds": 1.0,
                "memory_limit_mb": 256.0
            },
            "benchmark_config": {
                "sizes": [1000, 2000, 4000, 8000],
                "repetitions": 3,
                "warmup": 1,
                "per_run_timeout_ms": 3000,
                "allow_unsandboxed": true
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "ok");
        let fit = &resp["results"]["benchmark"]["empirical_fit"];
        assert!(fit["ambiguous"].as_bool().unwrap() || fit["model"].is_string());
    }

    #[test]
    fn test_benchmark_action_compilation_error() {
        if compiler::discover_compiler(None).is_err() {
            return;
        }

        let req = serde_json::json!({
            "schema_version": "1.1",
            "request_id": "bench-err",
            "action": "benchmark",
            "source": {
                "path": "syntax_error.cpp",
                "language": "cpp",
                "content": "int main() { syntax error here; return 0; }"
            }
        });

        let resp_str = handle_request(&req.to_string());
        let resp: serde_json::Value = serde_json::from_str(&resp_str).unwrap();

        assert_eq!(resp["status"], "error");
        assert_eq!(resp["errors"][0]["code"], "source_parse_error");
        assert!(!resp["diagnostics"].as_array().unwrap().is_empty());
    }
}
