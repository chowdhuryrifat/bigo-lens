//! Feasibility assessment against contest time and memory limits.

use crate::constraints::ConstraintSet;
use crate::models::{Assessment, Feasibility, FeasibilityStatus, MemoryResult, Workload};

const DEFAULT_OPS_PER_SECOND: f64 = 1.0e8;

/// Assess time feasibility based on maximum workload and time limit.
pub fn assess_time(workload: Option<&Workload>, constraints: &ConstraintSet) -> Assessment {
    let time_limit = match constraints.time_limit_seconds {
        Some(t) => t,
        None => {
            return Assessment {
                status: FeasibilityStatus::Unknown,
                reason: "no time limit specified".to_string(),
            };
        }
    };

    let wl = match workload {
        Some(w) if w.upper_bound.is_some() => w.upper_bound.unwrap(),
        _ => {
            return Assessment {
                status: FeasibilityStatus::Unknown,
                reason: "maximum time workload could not be determined".to_string(),
            };
        }
    };

    let allowed_ops = time_limit * DEFAULT_OPS_PER_SECOND;
    if wl > allowed_ops {
        Assessment {
            status: FeasibilityStatus::HighRisk,
            reason: format!(
                "estimated workload ({:.1e} ops) exceeds expected operations budget ({:.1e} ops at {:.1}s)",
                wl, allowed_ops, time_limit
            ),
        }
    } else if wl > 0.5 * allowed_ops {
        Assessment {
            status: FeasibilityStatus::Borderline,
            reason: format!(
                "estimated workload ({:.1e} ops) is close to expected operations budget ({:.1e} ops)",
                wl, allowed_ops
            ),
        }
    } else {
        Assessment {
            status: FeasibilityStatus::LikelySafe,
            reason: format!(
                "estimated workload ({:.1e} ops) is well within limit ({:.1e} ops)",
                wl, allowed_ops
            ),
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
        Some(b) => b,
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
    workload: Option<&Workload>,
    memory: &MemoryResult,
    constraints: &ConstraintSet,
) -> Feasibility {
    Feasibility {
        time: assess_time(workload, constraints),
        memory: assess_memory(memory, constraints),
    }
}
