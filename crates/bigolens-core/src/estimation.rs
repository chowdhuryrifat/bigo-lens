//! Memory estimation and storage models.

use crate::constraints::ConstraintSet;
use crate::models::{BoundStatus, MemoryItem, MemoryResult, StorageKind};

/// Evaluates static and dynamic memory items under the constraint set.
pub fn estimate_memory(items: &[MemoryItem], _constraints: &ConstraintSet) -> MemoryResult {
    let mut static_bytes: u64 = 0;
    let mut dynamic_bytes: u64 = 0;
    let mut dynamic_status = BoundStatus::Proven;

    for item in items {
        match item.kind {
            StorageKind::GlobalStatic | StorageKind::LocalArray => {
                if let Some(bytes) = item.exact_bytes {
                    static_bytes = static_bytes.saturating_add(bytes);
                }
            }
            StorageKind::VectorBuffer => {
                // Dynamic vector buffer
                if let Some(ref _expr_str) = item.symbolic_bytes {
                    // If we have an expression, we can evaluate it
                    // For now, if exact_bytes is present or evaluated:
                    if let Some(b) = item.exact_bytes {
                        dynamic_bytes = dynamic_bytes.saturating_add(b);
                        dynamic_status = dynamic_status.weakest(BoundStatus::Estimated);
                    } else {
                        dynamic_status = dynamic_status.weakest(BoundStatus::Estimated);
                    }
                }
            }
        }
    }

    let estimated_min = if dynamic_bytes > 0 || static_bytes > 0 {
        Some((static_bytes.saturating_add(dynamic_bytes)) as f64)
    } else {
        None
    };

    MemoryResult {
        static_storage_bytes: static_bytes,
        static_storage_saturated: false,
        largest_local_array_bytes: 0,
        estimated_minimum_bytes: estimated_min,
        estimated_minimum_status: dynamic_status,
        assumptions: vec![
            "process baseline, stack frames, standard-library internals and unmodelled heap allocations are not included; figures are lower bounds, not peak usage".to_string(),
        ],
        items: items.to_vec(),
    }
}
