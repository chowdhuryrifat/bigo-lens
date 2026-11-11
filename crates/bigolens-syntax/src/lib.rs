//! Multi-language AST parser mapping source code across languages into BigO Lens IR.

pub mod cpp;
pub mod python;

use bigolens_core::constraints::{evaluate_workload, ConstraintSet, WorkloadResult};
use bigolens_core::models::{BoundStatus, Confidence, Finding, FindingCategory, Quantity};

#[derive(Debug, thiserror::Error)]
pub enum SyntaxError {
    #[error("failed to parse source code")]
    ParseFailure,
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
}

/// Analyze source code across languages and extract complexity findings and dominant workload.
pub fn analyze_source(
    source: &str,
    language: &str,
    file_name: &str,
    constraints: &ConstraintSet,
) -> (Vec<Finding>, Option<WorkloadResult>) {
    let normalized_lang = language.trim().to_lowercase();
    let mut raw_findings = Vec::new();
    let mut candidate_workloads: Vec<WorkloadResult> = Vec::new();

    match normalized_lang.as_str() {
        "python" | "py" => {
            // 1. Python loops
            if let Ok(loops) = python::parse_python_loops(source, file_name) {
                for lp in loops {
                    let outcome = lp.analyze();
                    if let Some(ref total_cost) = outcome.total_cost {
                        let wl_res = evaluate_workload(total_cost, constraints);
                        candidate_workloads.push(wl_res.clone());

                        let time_qty = Quantity {
                            expression: total_cost.to_string(),
                            asymptotic: Some(total_cost.asymptotic_bound()),
                            variables: total_cost.free_variables().into_iter().collect(),
                            workload: wl_res.into(),
                        };

                        raw_findings.push(Finding {
                            id: String::new(),
                            category: FindingCategory::LoopNest,
                            pattern: outcome.pattern.clone(),
                            description: format!("loop nest with trip count {}", total_cost),
                            function_name: "main".to_string(),
                            location: lp.location,
                            time: Some(time_qty),
                            space: None,
                            bound_status: outcome.bound_status,
                            confidence: outcome.confidence,
                            assumptions: outcome.assumptions,
                            limitations: outcome.limitations,
                            provenance: "bigolens-syntax".to_string(),
                        });
                    }
                }
            }

            // 2. Python recursion
            if let Ok(recs) = python::parse_python_recursion(source, file_name) {
                for rec in recs {
                    let (cost_expr, status, confidence, note) = rec.solve();
                    let wl_res = evaluate_workload(&cost_expr, constraints);
                    candidate_workloads.push(wl_res.clone());

                    let time_qty = Quantity {
                        expression: cost_expr.to_string(),
                        asymptotic: Some(cost_expr.asymptotic_bound()),
                        variables: cost_expr.free_variables().into_iter().collect(),
                        workload: wl_res.into(),
                    };

                    let pattern = if rec.reduction_divisor > 1.0 {
                        "master-theorem".to_string()
                    } else {
                        "linear-decrement".to_string()
                    };

                    let limitations = if status == BoundStatus::Unsupported {
                        vec![note.clone()]
                    } else {
                        Vec::new()
                    };

                    raw_findings.push(Finding {
                        id: String::new(),
                        category: FindingCategory::Recursion,
                        pattern,
                        description: format!("recursive function '{}': {}", rec.name, note),
                        function_name: rec.name,
                        location: rec.location,
                        time: Some(time_qty),
                        space: None,
                        bound_status: status,
                        confidence,
                        assumptions: Vec::new(),
                        limitations,
                        provenance: "bigolens-syntax".to_string(),
                    });
                }
            }

            // 3. Python memory allocation
            if let Ok(mems) = python::parse_python_memory_allocations(source, file_name) {
                for mem in mems {
                    let wl_res = evaluate_workload(&mem.space_expr, constraints);

                    let space_qty = Quantity {
                        expression: mem.space_expr.to_string(),
                        asymptotic: Some(mem.space_expr.asymptotic_bound()),
                        variables: mem.space_expr.free_variables().into_iter().collect(),
                        workload: wl_res.into(),
                    };

                    raw_findings.push(Finding {
                        id: String::new(),
                        category: FindingCategory::DynamicAllocation,
                        pattern: mem.pattern,
                        description: mem.description,
                        function_name: mem.function_name,
                        location: mem.location,
                        time: None,
                        space: Some(space_qty),
                        bound_status: BoundStatus::Proven,
                        confidence: Confidence::High,
                        assumptions: Vec::new(),
                        limitations: Vec::new(),
                        provenance: "bigolens-syntax".to_string(),
                    });
                }
            }
        }
        "cpp" | "c++" => {
            if let Ok(loops) = cpp::parse_cpp_loops(source, file_name) {
                for lp in loops {
                    let outcome = lp.analyze();
                    if let Some(ref total_cost) = outcome.total_cost {
                        let wl_res = evaluate_workload(total_cost, constraints);
                        candidate_workloads.push(wl_res.clone());

                        let time_qty = Quantity {
                            expression: total_cost.to_string(),
                            asymptotic: Some(total_cost.asymptotic_bound()),
                            variables: total_cost.free_variables().into_iter().collect(),
                            workload: wl_res.into(),
                        };

                        raw_findings.push(Finding {
                            id: String::new(),
                            category: FindingCategory::LoopNest,
                            pattern: outcome.pattern.clone(),
                            description: format!("loop nest with trip count {}", total_cost),
                            function_name: "main".to_string(),
                            location: lp.location,
                            time: Some(time_qty),
                            space: None,
                            bound_status: outcome.bound_status,
                            confidence: outcome.confidence,
                            assumptions: outcome.assumptions,
                            limitations: outcome.limitations,
                            provenance: "bigolens-syntax".to_string(),
                        });
                    }
                }
            }
        }
        _ => return (Vec::new(), None),
    }

    // Sort findings in source order
    raw_findings.sort_by_key(|f| (f.location.line, f.location.column));

    // Assign sequential IDs: F1, F2, ...
    for (idx, f) in raw_findings.iter_mut().enumerate() {
        f.id = format!("F{}", idx + 1);
    }

    // Determine the maximum workload
    let mut max_workload: Option<WorkloadResult> = None;
    for wl in candidate_workloads {
        if let Some(ub) = wl.upper_bound {
            let is_larger = max_workload
                .as_ref()
                .and_then(|mw| mw.upper_bound)
                .is_none_or(|cur| ub > cur);
            if is_larger {
                max_workload = Some(wl);
            }
        } else if max_workload.is_none() {
            max_workload = Some(wl);
        }
    }

    (raw_findings, max_workload)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigolens_core::constraints::VariableBound;

    #[test]
    fn test_analyze_source_python() {
        let code = r#"
def process(n):
    data = [0] * n
    for i in range(n):
        print(i)
"#;
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(1000.0),
            },
        );

        let (findings, max_wl) = analyze_source(code, "python", "test.py", &constraints);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].id, "F1");
        assert_eq!(findings[1].id, "F2");

        let wl = max_wl.unwrap();
        assert_eq!(wl.upper_bound, Some(1000.0));
    }

    #[test]
    fn test_analyze_source_cpp() {
        let code = r#"
int main() {
    for (int i = 0; i < n; ++i) {
        for (int j = 0; j < i; ++j) {
            work();
        }
    }
}
"#;
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(100.0),
            },
        );

        let (findings, max_wl) = analyze_source(code, "cpp", "main.cpp", &constraints);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].pattern, "triangular-loop");
        assert_eq!(
            findings[0].time.as_ref().unwrap().asymptotic.as_deref(),
            Some("O(n^2)")
        );

        let wl = max_wl.unwrap();
        assert_eq!(wl.upper_bound, Some(10000.0));
    }

    #[test]
    fn test_analyze_source_unsupported_language() {
        let (findings, max_wl) = analyze_source(
            "hello",
            "unknown_lang",
            "file.xyz",
            &ConstraintSet::default(),
        );
        assert!(findings.is_empty());
        assert!(max_wl.is_none());
    }
}
