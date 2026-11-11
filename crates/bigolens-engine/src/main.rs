//! BigO Lens standalone engine binary.
//!
//! Provides the command-line interface and executes the versioned JSON protocol over stdin/stdout.

pub mod compiler;
pub mod runner;

use bigolens_core::api::{
    Request, ENGINE_NAME, ENGINE_VERSION, MAX_REQUEST_BYTES, PROTOCOL_VERSION,
};
use bigolens_core::models::{
    AnalyzeResults, BoundStatus, EngineMetadata, MemoryResult, ProtocolError, Response, Summary,
    Workload,
};
use clap::Parser;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process;

#[derive(Parser, Debug)]
#[command(name = "bigolens-engine")]
#[command(version = bigolens_core::api::ENGINE_VERSION)]
#[command(about = "BigO Lens static complexity and workload analyzer")]
struct Cli {
    /// Read JSON request from a file instead of standard input
    #[arg(long, value_name = "FILE")]
    request: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    let input = match cli.request {
        Some(path) => match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "bigolens-engine: failed to read request file '{:?}': {}",
                    path, e
                );
                process::exit(1);
            }
        },
        None => {
            let mut buffer = String::new();
            // Read up to 4 MiB + 1 byte to detect overflow
            let mut handle = io::stdin().take((MAX_REQUEST_BYTES + 1) as u64);
            if let Err(e) = handle.read_to_string(&mut buffer) {
                eprintln!("bigolens-engine: failed to read from stdin: {}", e);
                process::exit(1);
            }
            buffer
        }
    };

    let response = handle_request(&input);

    // Reserved stdout: exactly one line
    println!("{}", response);
}

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

fn dispatch_request(req: Request) -> Response {
    if req.schema_version != PROTOCOL_VERSION {
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

    match req.action.as_str() {
        "capabilities" => capabilities_response(req.request_id),
        "analyze" => analyze_action(req),
        _ => error_response(
            Some(req.request_id),
            "not_implemented",
            &format!("action '{}' is not implemented", req.action),
            Some("/action".to_string()),
        ),
    }
}

fn capabilities_response(request_id: String) -> Response {
    let caps = serde_json::json!({
        "actions": ["capabilities", "analyze", "benchmark"],
        "features": {
            "benchmark": "not_implemented",
            "constraint_workload": "partial",
            "interprocedural_analysis": "partial",
            "loop_analysis": "partial",
            "range_for_loops": "partial",
            "recurrence_extraction": "partial",
            "recursion_analysis": "partial",
            "static_memory_estimate": "partial",
            "stl_cost_model": "partial",
            "time_feasibility": "requires_runtime_estimate",
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

fn analyze_action(req: Request) -> Response {
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

    // 1. Multi-language complexity analysis via tree-sitter
    let (findings, max_workload) = bigolens_syntax::analyze_source(
        &source.content,
        &source.language,
        &source.path,
        &constraints,
    );

    // 2. Compute memory result
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

    // 3. Assess feasibility
    let max_workload_obj: Option<Workload> = max_workload.as_ref().map(|w| w.into());
    let feasibility = bigolens_core::feasibility::assess_feasibility(
        max_workload_obj.as_ref(),
        &memory,
        &constraints,
    );

    // 4. Identify dominant finding
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

fn engine_metadata() -> EngineMetadata {
    EngineMetadata {
        name: ENGINE_NAME.to_string(),
        version: ENGINE_VERSION.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
        frontend: Some("bigolens-syntax".to_string()),
    }
}

fn error_response(
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
        assert_eq!(resp["schema_version"], "1.0");

        let results = &resp["results"];
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
        assert_eq!(results["feasibility"]["time"]["status"], "likely_safe");
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
}
