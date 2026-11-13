//! Protocol data models conforming to shared/schemas/response.schema.json.

use crate::constraints::WorkloadResult;
use serde::{Deserialize, Serialize};

/// How strongly a reported bound is supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundStatus {
    Proven,
    Estimated,
    Unknown,
    Unsupported,
}

impl BoundStatus {
    /// Returns the weaker of two bound statuses.
    /// Proven > Estimated > Unknown > Unsupported.
    pub fn weakest(self, other: Self) -> Self {
        std::cmp::max(self, other)
    }
}

/// Subjective reliability of a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    None,
}

impl Confidence {
    /// Returns the weaker of two confidence levels.
    pub fn weakest(self, other: Self) -> Self {
        std::cmp::max(self, other)
    }
}

/// Feasibility verdict against contest limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeasibilityStatus {
    LikelySafe,
    Borderline,
    HighRisk,
    Unknown,
}

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Information,
}

/// 1-based source position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

/// A diagnostic message from the engine or frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub location: Option<SourceLocation>,
}

/// Protocol error object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Workload bound evaluated under constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workload {
    pub status: BoundStatus,
    pub upper_bound: Option<f64>,
    pub method: Option<String>,
    pub assumptions: Vec<String>,
    pub missing_bounds: Vec<String>,
}

impl From<WorkloadResult> for Workload {
    fn from(res: WorkloadResult) -> Self {
        Self {
            status: res.status,
            upper_bound: res.upper_bound,
            method: res.method,
            assumptions: res.assumptions,
            missing_bounds: res.missing_bounds,
        }
    }
}

impl From<&WorkloadResult> for Workload {
    fn from(res: &WorkloadResult) -> Self {
        Self {
            status: res.status,
            upper_bound: res.upper_bound,
            method: res.method.clone(),
            assumptions: res.assumptions.clone(),
            missing_bounds: res.missing_bounds.clone(),
        }
    }
}

/// Evaluated quantity (time or space).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quantity {
    pub expression: String,
    pub asymptotic: Option<String>,
    pub variables: Vec<String>,
    pub workload: Workload,
}

/// Category of an analysis finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingCategory {
    LoopNest,
    Recursion,
    StaticStorage,
    DynamicAllocation,
    UnsupportedConstruct,
}

/// A single explainable analysis finding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub category: FindingCategory,
    pub pattern: String,
    pub description: String,
    #[serde(rename = "function")]
    pub function_name: String,
    pub location: SourceLocation,
    pub time: Option<Quantity>,
    pub space: Option<Quantity>,
    pub bound_status: BoundStatus,
    pub confidence: Confidence,
    pub assumptions: Vec<String>,
    pub limitations: Vec<String>,
    pub provenance: String,
}

/// Feasibility assessment result with status and explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assessment {
    pub status: FeasibilityStatus,
    pub reason: String,
}

/// Feasibility assessments for time and memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feasibility {
    pub time: Assessment,
    pub memory: Assessment,
}

/// Storage item kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageKind {
    GlobalStatic,
    LocalArray,
    VectorBuffer,
}

/// A tracked memory allocation item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub name: String,
    pub kind: StorageKind,
    #[serde(rename = "type")]
    pub type_name: String,
    pub exact_bytes: Option<u64>,
    pub symbolic_bytes: Option<String>,
    pub location: SourceLocation,
}

/// Memory evaluation results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryResult {
    pub static_storage_bytes: u64,
    pub static_storage_saturated: bool,
    pub largest_local_array_bytes: u64,
    pub estimated_minimum_bytes: Option<f64>,
    pub estimated_minimum_status: BoundStatus,
    pub assumptions: Vec<String>,
    pub items: Vec<MemoryItem>,
}

/// Top-level summary of analysis results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub finding_count: usize,
    pub max_time_workload: Option<f64>,
    pub dominant_finding_id: Option<String>,
    pub coverage: String,
}

/// Results block of an analyze request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalyzeResults {
    pub findings: Vec<Finding>,
    pub memory: MemoryResult,
    pub feasibility: Feasibility,
    pub summary: Summary,
}

/// Engine metadata in response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineMetadata {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
    pub frontend: Option<String>,
}

/// Top-level response structure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub schema_version: String,
    pub request_id: Option<String>,
    pub status: String,
    pub engine: EngineMetadata,
    pub results: Option<serde_json::Value>,
    pub diagnostics: Vec<Diagnostic>,
    pub errors: Vec<ProtocolError>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bound_status_weakest() {
        assert_eq!(
            BoundStatus::Proven.weakest(BoundStatus::Proven),
            BoundStatus::Proven
        );
        assert_eq!(
            BoundStatus::Proven.weakest(BoundStatus::Estimated),
            BoundStatus::Estimated
        );
        assert_eq!(
            BoundStatus::Estimated.weakest(BoundStatus::Unknown),
            BoundStatus::Unknown
        );
        assert_eq!(
            BoundStatus::Unknown.weakest(BoundStatus::Unsupported),
            BoundStatus::Unsupported
        );
    }

    #[test]
    fn test_serde_serialization() {
        let loc = SourceLocation {
            file: "solution.cpp".to_string(),
            line: 10,
            column: 5,
            end_line: Some(15),
            end_column: Some(1),
        };
        let serialized = serde_json::to_string(&loc).unwrap();
        assert!(serialized.contains("\"file\":\"solution.cpp\""));
        assert!(serialized.contains("\"line\":10"));
        assert!(serialized.contains("\"end_line\":15"));
    }
}
