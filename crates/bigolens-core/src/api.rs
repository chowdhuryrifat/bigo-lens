//! JSON protocol request parsing, validation, and response construction.

use crate::constraints::ConstraintSet;
use crate::models::{
    AnalyzeResults, EngineMetadata, MemoryResult, ProtocolError, Response, Summary,
};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: &str = "1.1";
pub const ENGINE_VERSION: &str = "0.1.0";
pub const ENGINE_NAME: &str = "bigolens-engine";
pub const MAX_REQUEST_BYTES: usize = 4 * 1024 * 1024; // 4 MiB
pub const MAX_SOURCE_BYTES: usize = 2 * 1024 * 1024; // 2 MiB

/// Error returned when parsing schema versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid schema version format, expected MAJOR.MINOR")]
pub struct VersionParseError;

/// Check whether the request schema version is supported by the engine.
///
/// Follows JSON_PROTOCOL.md §2:
/// Accepts when MAJOR equals engine MAJOR and MINOR <= engine MINOR.
/// Returns Err(VersionParseError) when format is not MAJOR.MINOR.
pub fn is_supported_schema_version(
    req_version: &str,
    engine_version: &str,
) -> Result<bool, VersionParseError> {
    let mut req_parts = req_version.split('.');
    let req_major: u32 = req_parts
        .next()
        .ok_or(VersionParseError)?
        .parse()
        .map_err(|_| VersionParseError)?;
    let req_minor: u32 = req_parts
        .next()
        .ok_or(VersionParseError)?
        .parse()
        .map_err(|_| VersionParseError)?;
    if req_parts.next().is_some() {
        return Err(VersionParseError);
    }

    let mut eng_parts = engine_version.split('.');
    let eng_major: u32 = eng_parts
        .next()
        .ok_or(VersionParseError)?
        .parse()
        .map_err(|_| VersionParseError)?;
    let eng_minor: u32 = eng_parts
        .next()
        .ok_or(VersionParseError)?
        .parse()
        .map_err(|_| VersionParseError)?;

    Ok(req_major == eng_major && req_minor <= eng_minor)
}

/// Source file payload in an analyze or benchmark request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePayload {
    pub path: String,
    pub language: String,
    pub content: String,
}

/// Request options.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisOptions {
    pub cpp_standard: Option<String>,
}

/// Configuration options for dynamic benchmark execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    #[serde(default)]
    pub sizes: Vec<usize>,
    #[serde(default = "default_repetitions")]
    pub repetitions: usize,
    #[serde(default = "default_warmup")]
    pub warmup: usize,
    #[serde(default = "default_seed")]
    pub seed: u64,
    #[serde(default = "default_timeout_ms")]
    pub per_run_timeout_ms: u64,
    #[serde(default)]
    pub allow_unsandboxed: bool,
}

fn default_repetitions() -> usize {
    5
}
fn default_warmup() -> usize {
    1
}
fn default_seed() -> u64 {
    42
}
fn default_timeout_ms() -> u64 {
    1000
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            sizes: Vec::new(),
            repetitions: 5,
            warmup: 1,
            seed: 42,
            per_run_timeout_ms: 1000,
            allow_unsandboxed: false,
        }
    }
}

/// Incoming JSON protocol request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub schema_version: String,
    pub request_id: String,
    pub action: String,
    pub source: Option<SourcePayload>,
    pub constraints: Option<ConstraintSet>,
    pub options: Option<AnalysisOptions>,
    pub benchmark_config: Option<BenchmarkConfig>,
}

/// Central request handler.
pub struct RequestHandler {
    frontend_name: Option<String>,
}

impl Default for RequestHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestHandler {
    pub fn new() -> Self {
        Self {
            frontend_name: Some("clang-20".to_string()),
        }
    }

    pub fn with_frontend(frontend: Option<String>) -> Self {
        Self {
            frontend_name: frontend,
        }
    }

    /// Process raw text request from stdin or file and return a single response line.
    pub fn handle_text(&self, text: &str) -> String {
        if text.len() > MAX_REQUEST_BYTES {
            let resp = self.error_response(
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
                let resp = self.error_response(
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
                let resp = self.error_response(
                    request_id,
                    "invalid_request",
                    &format!("schema validation error: {}", e),
                    None,
                );
                return serde_json::to_string(&resp).unwrap();
            }
        };

        let resp = self.handle_request(req);
        serde_json::to_string(&resp).unwrap()
    }

    pub fn handle_request(&self, req: Request) -> Response {
        match is_supported_schema_version(&req.schema_version, PROTOCOL_VERSION) {
            Ok(true) => {}
            Ok(false) => {
                return self.error_response(
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
                return self.error_response(
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
            "capabilities" => self.capabilities_response(req.request_id),
            "analyze" => self.analyze_response(req),
            _ => self.error_response(
                Some(req.request_id),
                "not_implemented",
                &format!("action '{}' is not implemented", req.action),
                Some("/action".to_string()),
            ),
        }
    }

    fn capabilities_response(&self, request_id: String) -> Response {
        let caps = serde_json::json!({
            "actions": ["capabilities", "analyze", "benchmark"],
            "supported_languages": ["cpp"],
            "cpp_standards": ["c++11", "c++14", "c++17", "c++20", "c++23"],
            "frontend_available": self.frontend_name.is_some()
        });

        Response {
            schema_version: PROTOCOL_VERSION.to_string(),
            request_id: Some(request_id),
            status: "ok".to_string(),
            engine: self.engine_metadata(),
            results: Some(caps),
            diagnostics: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn analyze_response(&self, req: Request) -> Response {
        let constraints = req.constraints.unwrap_or_default();
        let memory = MemoryResult {
            static_storage_bytes: 0,
            static_storage_saturated: false,
            largest_local_array_bytes: 0,
            estimated_minimum_bytes: Some(0.0),
            estimated_minimum_status: crate::models::BoundStatus::Proven,
            assumptions: vec![
                "process baseline, stack frames, standard-library internals and unmodelled heap allocations are not included; figures are lower bounds, not peak usage".to_string(),
            ],
            items: Vec::new(),
        };

        let feasibility = crate::feasibility::assess_feasibility(None, &memory, &constraints);

        let summary = Summary {
            finding_count: 0,
            max_time_workload: None,
            dominant_finding_id: None,
            coverage: "partial".to_string(),
        };

        let results = AnalyzeResults {
            findings: Vec::new(),
            memory,
            feasibility,
            summary,
        };

        Response {
            schema_version: PROTOCOL_VERSION.to_string(),
            request_id: Some(req.request_id),
            status: "ok".to_string(),
            engine: self.engine_metadata(),
            results: Some(serde_json::to_value(results).unwrap()),
            diagnostics: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn engine_metadata(&self) -> EngineMetadata {
        EngineMetadata {
            name: ENGINE_NAME.to_string(),
            version: ENGINE_VERSION.to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
            frontend: self.frontend_name.clone(),
        }
    }

    fn error_response(
        &self,
        request_id: Option<String>,
        code: &str,
        message: &str,
        path: Option<String>,
    ) -> Response {
        Response {
            schema_version: PROTOCOL_VERSION.to_string(),
            request_id,
            status: "error".to_string(),
            engine: self.engine_metadata(),
            results: None,
            diagnostics: Vec::new(),
            errors: vec![ProtocolError {
                code: code.to_string(),
                message: message.to_string(),
                path,
            }],
        }
    }
}
