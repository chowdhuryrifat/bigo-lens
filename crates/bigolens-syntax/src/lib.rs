//! Multi-language AST parser mapping source code across languages into BigO Lens IR.

pub mod cpp;
pub mod java;
pub mod javascript;
pub mod python;
pub mod rust_lang;

use bigolens_core::complexity::ExprPtr;
use bigolens_core::constraints::{evaluate_workload, ConstraintSet, WorkloadResult};
use bigolens_core::ir::{LoopNestIR, RecursiveFunctionIR};
use bigolens_core::models::{
    BoundStatus, Confidence, Finding, FindingCategory, Quantity, SourceLocation,
};

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
            if let Ok(loops) = python::parse_python_loops(source, file_name) {
                process_loops(
                    loops,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(recs) = python::parse_python_recursion(source, file_name) {
                process_recursions(
                    recs,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(mems) = python::parse_python_memory_allocations(source, file_name) {
                let mapped = mems
                    .into_iter()
                    .map(|m| {
                        (
                            m.location,
                            m.space_expr,
                            m.pattern,
                            m.description,
                            m.function_name,
                        )
                    })
                    .collect();
                process_allocations(mapped, constraints, &mut raw_findings);
            }
        }
        "cpp" | "c++" => {
            if let Ok(loops) = cpp::parse_cpp_loops(source, file_name) {
                process_loops(
                    loops,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(recs) = cpp::parse_cpp_recursion(source, file_name) {
                process_recursions(
                    recs,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(mems) = cpp::parse_cpp_memory_allocations(source, file_name) {
                let mapped = mems
                    .into_iter()
                    .map(|m| {
                        (
                            m.location,
                            m.space_expr,
                            m.pattern,
                            m.description,
                            m.function_name,
                        )
                    })
                    .collect();
                process_allocations(mapped, constraints, &mut raw_findings);
            }
            if let Ok(stls) = cpp::parse_cpp_stl_calls(source, file_name) {
                for stl in stls {
                    let wl_res = evaluate_workload(&stl.cost_expr, constraints);
                    candidate_workloads.push(wl_res.clone());

                    let time_qty = Quantity {
                        expression: stl.cost_expr.to_string(),
                        asymptotic: Some(stl.asymptotic.clone()),
                        variables: stl.cost_expr.free_variables().into_iter().collect(),
                        workload: wl_res.into(),
                    };

                    raw_findings.push(Finding {
                        id: String::new(),
                        category: FindingCategory::LoopNest,
                        pattern: stl.pattern,
                        description: stl.description,
                        function_name: stl.function_name,
                        location: stl.location,
                        time: Some(time_qty),
                        space: None,
                        bound_status: BoundStatus::Proven,
                        confidence: Confidence::Medium,
                        assumptions: vec!["worst-case O(n log n) comparison sort".to_string()],
                        limitations: Vec::new(),
                        provenance: "bigolens-syntax".to_string(),
                    });
                }
            }
        }
        "java" => {
            if let Ok(loops) = java::parse_java_loops(source, file_name) {
                process_loops(
                    loops,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(recs) = java::parse_java_recursion(source, file_name) {
                process_recursions(
                    recs,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(mems) = java::parse_java_memory_allocations(source, file_name) {
                let mapped = mems
                    .into_iter()
                    .map(|m| {
                        (
                            m.location,
                            m.space_expr,
                            m.pattern,
                            m.description,
                            m.function_name,
                        )
                    })
                    .collect();
                process_allocations(mapped, constraints, &mut raw_findings);
            }
        }
        "rust" | "rs" => {
            if let Ok(loops) = rust_lang::parse_rust_loops(source, file_name) {
                process_loops(
                    loops,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(recs) = rust_lang::parse_rust_recursion(source, file_name) {
                process_recursions(
                    recs,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(mems) = rust_lang::parse_rust_memory_allocations(source, file_name) {
                let mapped = mems
                    .into_iter()
                    .map(|m| {
                        (
                            m.location,
                            m.space_expr,
                            m.pattern,
                            m.description,
                            m.function_name,
                        )
                    })
                    .collect();
                process_allocations(mapped, constraints, &mut raw_findings);
            }
        }
        "javascript" | "js" | "typescript" | "ts" => {
            if let Ok(loops) = javascript::parse_js_loops(source, file_name) {
                process_loops(
                    loops,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(recs) = javascript::parse_js_recursion(source, file_name) {
                process_recursions(
                    recs,
                    constraints,
                    &mut raw_findings,
                    &mut candidate_workloads,
                );
            }
            if let Ok(mems) = javascript::parse_js_memory_allocations(source, file_name) {
                let mapped = mems
                    .into_iter()
                    .map(|m| {
                        (
                            m.location,
                            m.space_expr,
                            m.pattern,
                            m.description,
                            m.function_name,
                        )
                    })
                    .collect();
                process_allocations(mapped, constraints, &mut raw_findings);
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

fn process_loops(
    loops: Vec<LoopNestIR>,
    constraints: &ConstraintSet,
    raw_findings: &mut Vec<Finding>,
    candidate_workloads: &mut Vec<WorkloadResult>,
) {
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

fn process_recursions(
    recursions: Vec<RecursiveFunctionIR>,
    constraints: &ConstraintSet,
    raw_findings: &mut Vec<Finding>,
    candidate_workloads: &mut Vec<WorkloadResult>,
) {
    for rec in recursions {
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

fn process_allocations(
    allocations: Vec<(SourceLocation, ExprPtr, String, String, String)>,
    constraints: &ConstraintSet,
    raw_findings: &mut Vec<Finding>,
) {
    for (location, space_expr, pattern, description, function_name) in allocations {
        let wl_res = evaluate_workload(&space_expr, constraints);
        let space_qty = Quantity {
            expression: space_expr.to_string(),
            asymptotic: Some(space_expr.asymptotic_bound()),
            variables: space_expr.free_variables().into_iter().collect(),
            workload: wl_res.into(),
        };

        raw_findings.push(Finding {
            id: String::new(),
            category: FindingCategory::DynamicAllocation,
            pattern,
            description,
            function_name,
            location,
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
    fn test_analyze_source_java() {
        let code = r#"
class Solution {
    public void run(int n) {
        int[] arr = new int[n];
        for (int i = 0; i < n; i++) {
            System.out.println(i);
        }
    }
}
"#;
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(500.0),
            },
        );

        let (findings, max_wl) = analyze_source(code, "java", "Solution.java", &constraints);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].category, FindingCategory::DynamicAllocation);
        assert_eq!(findings[1].category, FindingCategory::LoopNest);
        let wl = max_wl.unwrap();
        assert_eq!(wl.upper_bound, Some(500.0));
    }

    #[test]
    fn test_analyze_source_rust() {
        let code = r#"
fn solve(n: usize) {
    let buf = vec![0; n];
    for i in 0..n {
        println!("{}", i);
    }
}
"#;
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(250.0),
            },
        );

        let (findings, max_wl) = analyze_source(code, "rust", "main.rs", &constraints);
        assert_eq!(findings.len(), 2);
        let wl = max_wl.unwrap();
        assert_eq!(wl.upper_bound, Some(250.0));
    }

    #[test]
    fn test_analyze_source_javascript() {
        let code = r#"
function run(n) {
    const arr = new Array(n);
    for (let i = 0; i < n; i++) {
        console.log(i);
    }
}
"#;
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(400.0),
            },
        );

        let (findings, max_wl) = analyze_source(code, "javascript", "script.js", &constraints);
        assert_eq!(findings.len(), 2);
        let wl = max_wl.unwrap();
        assert_eq!(wl.upper_bound, Some(400.0));
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
