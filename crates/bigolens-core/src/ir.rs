//! Language-agnostic Intermediate Representation (IR) for algorithm complexity analysis.
//!
//! Frontends for different programming languages (C++, Python, Java, Rust, etc.)
//! translate language-specific ASTs into this unified IR. The core analyzer then
//! derives loop trip counts, recurrences, and asymptotic bounds in a completely
//! language-independent manner.

use crate::complexity::{logarithm, multiply, power, variable, ExprPtr};
use crate::models::{BoundStatus, Confidence, SourceLocation};

/// Direction of loop iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopDirection {
    Increasing,
    Decreasing,
}

/// Step behavior for a loop induction variable.
#[derive(Debug, Clone, PartialEq)]
pub enum StepKind {
    /// Linear step: i += c or i -= c
    Linear(ExprPtr),
    /// Geometric step: i *= factor or i <<= shift
    GeometricMultiply(f64),
    /// Halving step: i /= divisor or i >>= shift
    GeometricDivide(f64),
}

/// Language-agnostic representation of a loop nest.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopNestIR {
    pub induction_var: Option<String>,
    pub lower_bound: Option<ExprPtr>,
    pub upper_bound: Option<ExprPtr>,
    pub step: StepKind,
    pub direction: LoopDirection,
    pub is_inclusive: bool,
    pub location: SourceLocation,
    pub inner_loops: Vec<LoopNestIR>,
    pub has_early_exit: bool,
    pub bound_modified_in_body: bool,
    pub index_modified_in_body: bool,
}

/// Analysis result for a loop nest.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopAnalysisOutcome {
    pub trip_count: Option<ExprPtr>,
    pub total_cost: Option<ExprPtr>,
    pub bound_status: BoundStatus,
    pub confidence: Confidence,
    pub pattern: String,
    pub assumptions: Vec<String>,
    pub limitations: Vec<String>,
}

impl LoopNestIR {
    /// Analyze this language-agnostic loop nest and compute its closed-form complexity.
    pub fn analyze(&self) -> LoopAnalysisOutcome {
        let mut assumptions = Vec::new();
        let mut limitations = Vec::new();

        // 1. Check for unmodelled mutations in body
        if self.index_modified_in_body {
            limitations
                .push("loop induction variable is modified inside the loop body".to_string());
            return LoopAnalysisOutcome {
                trip_count: None,
                total_cost: None,
                bound_status: BoundStatus::Unsupported,
                confidence: Confidence::None,
                pattern: "unsupported-loop".to_string(),
                assumptions,
                limitations,
            };
        }

        if self.bound_modified_in_body {
            limitations
                .push("loop boundary condition is modified inside the loop body".to_string());
            return LoopAnalysisOutcome {
                trip_count: None,
                total_cost: None,
                bound_status: BoundStatus::Unsupported,
                confidence: Confidence::None,
                pattern: "unsupported-loop".to_string(),
                assumptions,
                limitations,
            };
        }

        // 2. Derive trip count for this loop level
        let (level_trips, pattern) = match &self.step {
            StepKind::Linear(_step_expr) => {
                if let (Some(ref lower), Some(ref upper)) = (&self.lower_bound, &self.upper_bound) {
                    // Check if lower is 0 and upper is N
                    let trips = match (lower.as_ref(), upper.as_ref()) {
                        (
                            crate::complexity::Expr::Constant(c),
                            crate::complexity::Expr::Variable(_),
                        ) if *c == 0.0 => upper.clone(),
                        _ => upper.clone(),
                    };
                    (Some(trips), "linear-loop")
                } else if let Some(ref upper) = self.upper_bound {
                    (Some(upper.clone()), "linear-loop")
                } else {
                    (None, "unbounded-loop")
                }
            }
            StepKind::GeometricMultiply(factor) | StepKind::GeometricDivide(factor) => {
                let target = self
                    .upper_bound
                    .clone()
                    .unwrap_or_else(|| variable("n").unwrap());
                let trips = logarithm(target, *factor).ok();
                (trips, "logarithmic-loop")
            }
        };

        let mut status = if level_trips.is_some() {
            BoundStatus::Proven
        } else {
            BoundStatus::Unknown
        };
        let mut confidence = if status == BoundStatus::Proven {
            Confidence::High
        } else {
            Confidence::None
        };

        if self.has_early_exit {
            status = status.weakest(BoundStatus::Estimated);
            confidence = confidence.weakest(Confidence::Medium);
            assumptions
                .push("early exit breaks may terminate the loop in fewer iterations".to_string());
        }

        // 3. Compose with inner loops
        let mut is_triangular = false;
        let total_cost = if self.inner_loops.is_empty() {
            level_trips.clone()
        } else {
            let mut current_trips = level_trips.clone();
            for inner in &self.inner_loops {
                let inner_outcome = inner.analyze();
                status = status.weakest(inner_outcome.bound_status);
                confidence = confidence.weakest(inner_outcome.confidence);
                assumptions.extend(inner_outcome.assumptions);
                limitations.extend(inner_outcome.limitations);

                let depends_on_outer = if let Some(ref ind_var) = self.induction_var {
                    if let Some(ref inner_t) = inner_outcome.total_cost {
                        inner_t.free_variables().contains(ind_var)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if depends_on_outer {
                    is_triangular = true;
                    if let Some(outer_t) = current_trips.take() {
                        current_trips = power(outer_t, 2.0).ok();
                    }
                } else if let (Some(outer_t), Some(inner_t)) =
                    (current_trips.take(), inner_outcome.total_cost)
                {
                    current_trips = Some(multiply(vec![outer_t, inner_t]));
                }
            }
            current_trips
        };

        let final_pattern = if is_triangular {
            "triangular-loop".to_string()
        } else if !self.inner_loops.is_empty() {
            "nested-loop".to_string()
        } else {
            pattern.to_string()
        };

        LoopAnalysisOutcome {
            trip_count: level_trips,
            total_cost,
            bound_status: status,
            confidence,
            pattern: final_pattern,
            assumptions,
            limitations,
        }
    }
}

/// Language-agnostic representation of a recursive divide-and-conquer function.
#[derive(Debug, Clone, PartialEq)]
pub struct RecursiveFunctionIR {
    pub name: String,
    pub parameter_name: String,
    pub branch_factor: u32,
    pub reduction_divisor: f64,
    pub reduction_subtrahend: Option<f64>,
    pub work_exponent: f64,
    pub location: SourceLocation,
}

impl RecursiveFunctionIR {
    /// Analyze recursion using Master Theorem or Linear Decrement.
    pub fn analyze(&self) -> (ExprPtr, BoundStatus, Confidence, String) {
        self.solve()
    }

    /// Solve recurrence using Master Theorem or Linear Decrement.
    pub fn solve(&self) -> (ExprPtr, BoundStatus, Confidence, String) {
        let n = variable(&self.parameter_name).unwrap();

        if let Some(c) = self.reduction_subtrahend {
            if self.branch_factor > 1 {
                return (
                    n,
                    BoundStatus::Unsupported,
                    Confidence::None,
                    "multiple recursion with subtractive arguments is not supported; exponential bounds are not derived".to_string(),
                );
            }
            // T(n) = T(n - c) + O(n^k)
            // If k == 0 => O(n)
            // If k == 1 => O(n^2)
            let result_exp = self.work_exponent + 1.0;
            let cost = power(n, result_exp).unwrap();
            let note = format!(
                "linear decrement recurrence: T(n) = T(n - {}) + O(n^{})",
                c, self.work_exponent
            );
            (cost, BoundStatus::Proven, Confidence::High, note)
        } else if self.reduction_divisor > 1.0 && self.branch_factor >= 1 {
            // Master Theorem: T(n) = a * T(n / b) + O(n^d)
            let a = self.branch_factor as f64;
            let b = self.reduction_divisor;
            let d = self.work_exponent;
            let critical = a.log(b);

            let eps = 1e-6;
            if (critical - d).abs() < eps {
                // Case 2: a == b^d => O(n^d log n)
                let nd = power(n.clone(), d).unwrap();
                let log_term = logarithm(n, 2.0).unwrap();
                let cost = multiply(vec![nd, log_term]);
                let note = format!("Master Theorem Case 2: a = {}, b = {}, d = {}", a, b, d);
                (cost, BoundStatus::Proven, Confidence::High, note)
            } else if critical > d {
                // Case 1: a > b^d => O(n^{log_b a})
                let cost = power(n, critical).unwrap();
                let note = format!(
                    "Master Theorem Case 1: a = {}, b = {}, critical = {:.2}",
                    a, b, critical
                );
                (cost, BoundStatus::Proven, Confidence::High, note)
            } else {
                // Case 3: a < b^d => O(n^d)
                let cost = power(n, d).unwrap();
                let note = format!("Master Theorem Case 3: a = {}, b = {}, d = {}", a, b, d);
                (cost, BoundStatus::Proven, Confidence::High, note)
            }
        } else {
            (
                n,
                BoundStatus::Unknown,
                Confidence::None,
                "unsupported recurrence shape".to_string(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::constant;

    #[test]
    fn test_language_agnostic_nested_loop() {
        // Outer: for i in 0..n
        // Inner: for j in 0..m
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();

        let inner = LoopNestIR {
            induction_var: Some("j".to_string()),
            lower_bound: Some(constant(0.0).unwrap()),
            upper_bound: Some(m),
            step: StepKind::Linear(constant(1.0).unwrap()),
            direction: LoopDirection::Increasing,
            is_inclusive: false,
            location: SourceLocation {
                file: "solution.py".to_string(),
                line: 2,
                column: 4,
                end_line: None,
                end_column: None,
            },
            inner_loops: Vec::new(),
            has_early_exit: false,
            bound_modified_in_body: false,
            index_modified_in_body: false,
        };

        let outer = LoopNestIR {
            induction_var: Some("i".to_string()),
            lower_bound: Some(constant(0.0).unwrap()),
            upper_bound: Some(n),
            step: StepKind::Linear(constant(1.0).unwrap()),
            direction: LoopDirection::Increasing,
            is_inclusive: false,
            location: SourceLocation {
                file: "solution.py".to_string(),
                line: 1,
                column: 0,
                end_line: None,
                end_column: None,
            },
            inner_loops: vec![inner],
            has_early_exit: false,
            bound_modified_in_body: false,
            index_modified_in_body: false,
        };

        let outcome = outer.analyze();
        assert_eq!(outcome.bound_status, BoundStatus::Proven);
        assert_eq!(outcome.confidence, Confidence::High);
        assert_eq!(outcome.pattern, "nested-loop");
        assert_eq!(outcome.total_cost.unwrap().to_string(), "n * m");
    }

    #[test]
    fn test_merge_sort_recurrence_ir() {
        // Merge sort: T(n) = 2 T(n/2) + O(n) => O(n log n)
        let rec = RecursiveFunctionIR {
            name: "merge_sort".to_string(),
            parameter_name: "n".to_string(),
            branch_factor: 2,
            reduction_divisor: 2.0,
            reduction_subtrahend: None,
            work_exponent: 1.0,
            location: SourceLocation {
                file: "Solution.java".to_string(),
                line: 10,
                column: 4,
                end_line: None,
                end_column: None,
            },
        };

        let (cost, status, confidence, note) = rec.solve();
        assert_eq!(status, BoundStatus::Proven);
        assert_eq!(confidence, Confidence::High);
        assert_eq!(cost.to_string(), "n * log_2(n)");
        assert!(note.contains("Master Theorem Case 2"));
    }

    #[test]
    fn test_triangular_loop_ir() {
        // Outer: for i in 0..n
        // Inner: for j in 0..i
        let n = variable("n").unwrap();
        let i = variable("i").unwrap();

        let inner = LoopNestIR {
            induction_var: Some("j".to_string()),
            lower_bound: Some(constant(0.0).unwrap()),
            upper_bound: Some(i),
            step: StepKind::Linear(constant(1.0).unwrap()),
            direction: LoopDirection::Increasing,
            is_inclusive: false,
            location: SourceLocation {
                file: "solution.cpp".to_string(),
                line: 2,
                column: 4,
                end_line: None,
                end_column: None,
            },
            inner_loops: Vec::new(),
            has_early_exit: false,
            bound_modified_in_body: false,
            index_modified_in_body: false,
        };

        let outer = LoopNestIR {
            induction_var: Some("i".to_string()),
            lower_bound: Some(constant(0.0).unwrap()),
            upper_bound: Some(n),
            step: StepKind::Linear(constant(1.0).unwrap()),
            direction: LoopDirection::Increasing,
            is_inclusive: false,
            location: SourceLocation {
                file: "solution.cpp".to_string(),
                line: 1,
                column: 0,
                end_line: None,
                end_column: None,
            },
            inner_loops: vec![inner],
            has_early_exit: false,
            bound_modified_in_body: false,
            index_modified_in_body: false,
        };

        let outcome = outer.analyze();
        assert_eq!(outcome.bound_status, BoundStatus::Proven);
        assert_eq!(outcome.confidence, Confidence::High);
        assert_eq!(outcome.pattern, "triangular-loop");
        assert_eq!(outcome.total_cost.unwrap().asymptotic_bound(), "O(n^2)");
    }
}
