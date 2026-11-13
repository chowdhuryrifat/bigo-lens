//! Feasibility assessment against contest time and memory limits.
//!
//! Follows FEASIBILITY_MODEL.md §1-§4.

use crate::constraints::ConstraintSet;
use crate::models::{Assessment, Feasibility, FeasibilityStatus, MemoryResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Empirical runtime extrapolation estimate at target constraint sizes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEstimate {
    /// Target variable sizes (e.g. {"n": 200000.0}).
    pub target_size: BTreeMap<String, f64>,
    /// Optimistic (lower) runtime estimate in milliseconds.
    pub min_ms: f64,
    /// Central runtime estimate in milliseconds.
    pub central_ms: f64,
    /// Pessimistic (upper) runtime estimate in milliseconds.
    pub max_ms: f64,
    /// Whether target size exceeded the largest measured input size.
    pub extrapolated: bool,
}

/// Assess time feasibility against contest time limits.
///
/// Follows the policy in FEASIBILITY_MODEL.md §3:
/// - Order 1: No time limit -> Unknown ("no time limit configured")
/// - Order 2: No runtime estimate -> Unknown ("no runtime estimate available; structural workload alone does not determine runtime")
/// - Order 3: low_ms > limit -> HighRisk ("even the optimistic runtime estimate exceeds the time limit")
/// - Order 4: high_ms <= 0.5 * limit -> LikelySafe ("the pessimistic runtime estimate is well within the time limit")
/// - Order 5: otherwise -> Borderline ("the runtime range overlaps the region near the time limit")
pub fn assess_time(
    runtime_estimate: Option<&RuntimeEstimate>,
    constraints: &ConstraintSet,
) -> Assessment {
    let time_limit_sec = match constraints.time_limit_seconds {
        Some(t) => t,
        None => {
            return Assessment {
                status: FeasibilityStatus::Unknown,
                reason: "no time limit configured".to_string(),
            };
        }
    };

    let limit_ms = time_limit_sec * 1000.0;

    let est = match runtime_estimate {
        Some(e) => e,
        None => {
            return Assessment {
                status: FeasibilityStatus::Unknown,
                reason: "no runtime estimate available; structural workload alone does not determine runtime".to_string(),
            };
        }
    };

    if est.min_ms > limit_ms {
        Assessment {
            status: FeasibilityStatus::HighRisk,
            reason: "even the optimistic runtime estimate exceeds the time limit".to_string(),
        }
    } else if est.max_ms <= 0.5 * limit_ms {
        Assessment {
            status: FeasibilityStatus::LikelySafe,
            reason: "the pessimistic runtime estimate is well within the time limit".to_string(),
        }
    } else {
        Assessment {
            status: FeasibilityStatus::Borderline,
            reason: "the runtime range overlaps the region near the time limit".to_string(),
        }
    }
}

/// Assess memory feasibility based on estimated storage and memory limit.
pub fn assess_memory(memory: &MemoryResult, constraints: &ConstraintSet) -> Assessment {
    let limit_mb = match constraints.memory_limit_mb {
        Some(m) => m,
        None => {
            return Assessment {
                status: FeasibilityStatus::Unknown,
                reason: "no memory limit specified".to_string(),
            };
        }
    };

    let limit_bytes = (limit_mb * 1024.0 * 1024.0) as u64;

    let min_bytes = match memory.estimated_minimum_bytes {
        Some(b) => b as u64,
        None => {
            return Assessment {
                status: FeasibilityStatus::LikelySafe,
                reason: "no significant storage allocations detected".to_string(),
            };
        }
    };

    if min_bytes > limit_bytes {
        Assessment {
            status: FeasibilityStatus::HighRisk,
            reason: format!(
                "evaluated minimum storage ({} bytes) exceeds the memory limit of {} MB ({} bytes)",
                min_bytes, limit_mb, limit_bytes
            ),
        }
    } else if min_bytes as f64 > 0.8 * limit_bytes as f64 {
        Assessment {
            status: FeasibilityStatus::Borderline,
            reason: format!(
                "storage ({} bytes) approaches the memory limit of {} MB",
                min_bytes, limit_mb
            ),
        }
    } else {
        Assessment {
            status: FeasibilityStatus::LikelySafe,
            reason: format!(
                "storage ({} bytes) is within the memory limit of {} MB",
                min_bytes, limit_mb
            ),
        }
    }
}

/// Full feasibility assessment.
pub fn assess_feasibility(
    runtime_estimate: Option<&RuntimeEstimate>,
    memory: &MemoryResult,
    constraints: &ConstraintSet,
) -> Feasibility {
    Feasibility {
        time: assess_time(runtime_estimate, constraints),
        memory: assess_memory(memory, constraints),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_feasibility_no_limit() {
        let constraints = ConstraintSet::default();
        let assessment = assess_time(None, &constraints);
        assert_eq!(assessment.status, FeasibilityStatus::Unknown);
        assert_eq!(assessment.reason, "no time limit configured");
    }

    #[test]
    fn test_time_feasibility_no_runtime_estimate() {
        let constraints = ConstraintSet {
            time_limit_seconds: Some(1.0),
            ..Default::default()
        };
        let assessment = assess_time(None, &constraints);
        assert_eq!(assessment.status, FeasibilityStatus::Unknown);
        assert_eq!(
            assessment.reason,
            "no runtime estimate available; structural workload alone does not determine runtime"
        );
    }

    #[test]
    fn test_time_feasibility_high_risk() {
        let constraints = ConstraintSet {
            time_limit_seconds: Some(1.0), // 1000 ms
            ..Default::default()
        };
        let est = RuntimeEstimate {
            target_size: BTreeMap::new(),
            min_ms: 1200.0,
            central_ms: 1500.0,
            max_ms: 1800.0,
            extrapolated: true,
        };
        let assessment = assess_time(Some(&est), &constraints);
        assert_eq!(assessment.status, FeasibilityStatus::HighRisk);
        assert_eq!(
            assessment.reason,
            "even the optimistic runtime estimate exceeds the time limit"
        );
    }

    #[test]
    fn test_time_feasibility_likely_safe() {
        let constraints = ConstraintSet {
            time_limit_seconds: Some(1.0), // 1000 ms, margin 500 ms
            ..Default::default()
        };
        let est = RuntimeEstimate {
            target_size: BTreeMap::new(),
            min_ms: 200.0,
            central_ms: 300.0,
            max_ms: 450.0, // <= 0.5 * 1000 ms
            extrapolated: true,
        };
        let assessment = assess_time(Some(&est), &constraints);
        assert_eq!(assessment.status, FeasibilityStatus::LikelySafe);
        assert_eq!(
            assessment.reason,
            "the pessimistic runtime estimate is well within the time limit"
        );
    }

    #[test]
    fn test_time_feasibility_borderline() {
        let constraints = ConstraintSet {
            time_limit_seconds: Some(1.0), // 1000 ms
            ..Default::default()
        };
        let est = RuntimeEstimate {
            target_size: BTreeMap::new(),
            min_ms: 400.0,
            central_ms: 600.0,
            max_ms: 800.0, // > 500 ms, <= 1000 ms
            extrapolated: true,
        };
        let assessment = assess_time(Some(&est), &constraints);
        assert_eq!(assessment.status, FeasibilityStatus::Borderline);
        assert_eq!(
            assessment.reason,
            "the runtime range overlaps the region near the time limit"
        );
    }
}
