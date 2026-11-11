//! Recurrence relation solving: Master Theorem and Linear Decrement.
//!
//! Conforms strictly to ADR 0012, COMPLEXITY_MODEL.md (§9), and RULES.md.

use crate::complexity::{constant, logarithm, multiply, power, variable, Expr, ExprPtr};
use crate::models::BoundStatus;
use std::sync::Arc;

/// Direction or kind of parameter reduction in a recursive self-call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeChange {
    /// Linear decrement: n -> n - amount (amount > 0)
    Decrement,
    /// Geometric division: n -> n / amount (amount > 1)
    Divide,
    /// Unrecognised or non-shrinking argument
    Unknown,
}

/// Specification of a single recursive self-call site.
#[derive(Debug, Clone, PartialEq)]
pub struct RecursiveCallSpec {
    pub change: SizeChange,
    pub amount: f64,
    pub inside_loop: bool,
}

/// Specification of a recurrence relation.
///
/// Models divide-and-conquer recurrences:
///   T(n) = a * T(n / b) + Theta(n^c * (log n)^k)
/// and linear decrement recurrences:
///   T(n) = T(n - c) + Theta(n^k)
#[derive(Debug, Clone, PartialEq)]
pub struct RecurrenceSpec {
    pub function_name: Option<String>,
    pub variable: String,
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub c_subtrahend: Option<f64>,
    pub c_work: f64,
    pub k_work_log: f64,
    pub calls: Vec<RecursiveCallSpec>,
    pub max_calls_per_invocation: Option<usize>,
    pub work_expr: Option<ExprPtr>,
}

impl RecurrenceSpec {
    /// Create a divide-and-conquer recurrence:
    /// T(n) = a * T(n / b) + Theta(n^c * (log n)^k)
    pub fn divide_and_conquer(a: f64, b: f64, c: f64, k: f64) -> Self {
        let count = if a.is_finite() && a >= 1.0 {
            a.round() as usize
        } else {
            1
        };
        let calls = vec![
            RecursiveCallSpec {
                change: SizeChange::Divide,
                amount: b,
                inside_loop: false,
            };
            count
        ];
        Self {
            function_name: None,
            variable: "n".to_string(),
            a: Some(a),
            b: Some(b),
            c_subtrahend: None,
            c_work: c,
            k_work_log: k,
            calls,
            max_calls_per_invocation: Some(count),
            work_expr: None,
        }
    }

    /// Create a linear decrement recurrence:
    /// T(n) = T(n - c) + Theta(n^k)
    pub fn linear_decrement(c: f64, k: f64) -> Self {
        Self {
            function_name: None,
            variable: "n".to_string(),
            a: Some(1.0),
            b: None,
            c_subtrahend: Some(c),
            c_work: k,
            k_work_log: 0.0,
            calls: vec![RecursiveCallSpec {
                change: SizeChange::Decrement,
                amount: c,
                inside_loop: false,
            }],
            max_calls_per_invocation: Some(1),
            work_expr: None,
        }
    }

    /// Create a multiple subtractive recurrence (e.g. Fibonacci):
    /// T(n) = T(n - d1) + T(n - d2) + ... + Theta(n^k)
    pub fn multiple_subtractive(decrements: Vec<f64>, k: f64) -> Self {
        let count = decrements.len();
        let calls = decrements
            .into_iter()
            .map(|amount| RecursiveCallSpec {
                change: SizeChange::Decrement,
                amount,
                inside_loop: false,
            })
            .collect();
        Self {
            function_name: None,
            variable: "n".to_string(),
            a: Some(count as f64),
            b: None,
            c_subtrahend: None,
            c_work: k,
            k_work_log: 0.0,
            calls,
            max_calls_per_invocation: Some(count),
            work_expr: None,
        }
    }

    pub fn with_variable(mut self, var: impl Into<String>) -> Self {
        self.variable = var.into();
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.function_name = Some(name.into());
        self
    }
}

/// Solution to an analyzed recurrence relation.
#[derive(Debug, Clone, PartialEq)]
pub struct RecurrenceSolution {
    pub status: BoundStatus,
    pub time: ExprPtr,
    pub asymptotic: String,
    pub method: String,
    pub assumptions: Vec<String>,
    pub limitations: Vec<String>,
}

impl RecurrenceSolution {
    pub fn is_proven(&self) -> bool {
        self.status == BoundStatus::Proven
    }
}

/// Trait implemented by recurrence solvers.
pub trait RecurrenceSolver: Send + Sync {
    fn name(&self) -> &'static str;
    fn solve(&self, spec: &RecurrenceSpec) -> Option<RecurrenceSolution>;
}

/// Solves divide-and-conquer recurrences using the Master Theorem:
/// T(n) = a * T(n / b) + Theta(n^c * (log n)^k)
#[derive(Debug, Default, Clone, Copy)]
pub struct MasterTheoremSolver;

impl RecurrenceSolver for MasterTheoremSolver {
    fn name(&self) -> &'static str {
        "master-theorem"
    }

    fn solve(&self, spec: &RecurrenceSpec) -> Option<RecurrenceSolution> {
        // If this recurrence is exclusively subtractive, Master Theorem does not apply.
        let is_decrement = spec.c_subtrahend.is_some()
            || (!spec.calls.is_empty()
                && spec.calls.iter().all(|c| c.change == SizeChange::Decrement));
        if is_decrement && spec.b.is_none() {
            // Check if multiple subtractive
            let decr_count = spec
                .calls
                .iter()
                .filter(|c| c.change == SizeChange::Decrement)
                .count();
            if decr_count > 1 || spec.max_calls_per_invocation.unwrap_or(0) > 1 {
                return Some(RecurrenceSolution {
                    status: BoundStatus::Unsupported,
                    time: constant(0.0).unwrap(),
                    asymptotic: "unsupported".to_string(),
                    method: self.name().to_string(),
                    assumptions: Vec::new(),
                    limitations: vec![
                        "multiple recursion with subtractive arguments is not supported; exponential bounds are not derived".to_string(),
                    ],
                });
            }
            return None;
        }

        let a = match spec.a {
            Some(val) => val,
            None => {
                let div_calls = spec
                    .calls
                    .iter()
                    .filter(|c| c.change == SizeChange::Divide)
                    .count();
                if div_calls > 0 {
                    div_calls as f64
                } else {
                    return None;
                }
            }
        };

        let b = match spec.b {
            Some(val) => val,
            None => {
                let div_call = spec.calls.iter().find(|c| c.change == SizeChange::Divide)?;
                div_call.amount
            }
        };

        // Check for non-standard / unsupported conditions
        if !a.is_finite() || a < 1.0 {
            return Some(RecurrenceSolution {
                status: BoundStatus::Unsupported,
                time: constant(0.0).unwrap(),
                asymptotic: "unsupported".to_string(),
                method: self.name().to_string(),
                assumptions: Vec::new(),
                limitations: vec![format!(
                    "Master Theorem requires constant branch factor a >= 1; received a = {}",
                    a
                )],
            });
        }

        if !b.is_finite() || b <= 1.0 {
            return Some(RecurrenceSolution {
                status: BoundStatus::Unsupported,
                time: constant(0.0).unwrap(),
                asymptotic: "unsupported".to_string(),
                method: self.name().to_string(),
                assumptions: Vec::new(),
                limitations: vec![format!(
                    "Master Theorem requires reduction divisor b > 1.0; received b = {}",
                    b
                )],
            });
        }

        // Check for mismatched divisors or calls inside loops
        for call in &spec.calls {
            if call.inside_loop {
                return Some(RecurrenceSolution {
                    status: BoundStatus::Unknown,
                    time: variable(&spec.variable).unwrap(),
                    asymptotic: "unknown".to_string(),
                    method: self.name().to_string(),
                    assumptions: Vec::new(),
                    limitations: vec![
                        "recursive self-call inside a loop is not supported".to_string()
                    ],
                });
            }
            if call.change == SizeChange::Divide && (call.amount - b).abs() > 1e-6 {
                return Some(RecurrenceSolution {
                    status: BoundStatus::Unsupported,
                    time: variable(&spec.variable).unwrap(),
                    asymptotic: "unsupported".to_string(),
                    method: self.name().to_string(),
                    assumptions: Vec::new(),
                    limitations: vec!["calls have mismatched divisors".to_string()],
                });
            }
        }

        let var = &spec.variable;
        let c = spec.c_work;
        let k = spec.k_work_log;

        // Critical exponent: log_b(a)
        let c_crit = a.ln() / b.ln();
        let eps = 1e-6;

        let (time, asymptotic) = if (c - c_crit).abs() < eps {
            // Case 2: c == log_b(a) => T(n) = Theta(n^(log_b(a)) * (log n)^(k + 1))
            let log_p = k + 1.0;
            let log_term = logarithm(variable(var).unwrap(), 2.0).unwrap();

            let time_expr = if c.abs() < eps {
                // c == 0 => log^(k+1)(n)
                if (log_p - 1.0).abs() < eps {
                    log_term
                } else {
                    power(log_term, log_p).unwrap()
                }
            } else if (c - 1.0).abs() < eps {
                // c == 1 => n * log^(k+1)(n)
                if (log_p - 1.0).abs() < eps {
                    multiply(vec![variable(var).unwrap(), log_term])
                } else {
                    multiply(vec![
                        variable(var).unwrap(),
                        power(log_term, log_p).unwrap(),
                    ])
                }
            } else {
                let n_c = power(variable(var).unwrap(), c).unwrap();
                if (log_p - 1.0).abs() < eps {
                    multiply(vec![n_c, log_term])
                } else {
                    multiply(vec![n_c, power(log_term, log_p).unwrap()])
                }
            };

            let asymp = if c.abs() < eps {
                if (log_p - 1.0).abs() < eps {
                    "Theta(log n)".to_string()
                } else {
                    format!("Theta((log n)^{})", log_p)
                }
            } else if (c - 1.0).abs() < eps {
                if (log_p - 1.0).abs() < eps {
                    "Theta(n log n)".to_string()
                } else {
                    format!("Theta(n (log n)^{})", log_p)
                }
            } else {
                format!("Theta(n^{} (log n)^{})", format_exponent(c), log_p)
            };

            (time_expr, asymp)
        } else if c < c_crit - eps {
            // Case 1: c < log_b(a) => T(n) = Theta(n^(log_b(a)))
            let rounded_exp = round_exponent(c_crit);
            let time_expr = if rounded_exp.abs() < eps {
                constant(1.0).unwrap()
            } else if (rounded_exp - 1.0).abs() < eps {
                variable(var).unwrap()
            } else {
                power(variable(var).unwrap(), rounded_exp).unwrap()
            };

            let asymp = if rounded_exp.abs() < eps {
                "Theta(1)".to_string()
            } else if (rounded_exp - 1.0).abs() < eps {
                "Theta(n)".to_string()
            } else {
                format!("Theta(n^{})", format_exponent(rounded_exp))
            };

            (time_expr, asymp)
        } else {
            // Case 3: c > log_b(a) with regularity condition => T(n) = Theta(n^c * (log n)^k)
            // Regularity condition a * (n/b)^c <= d * n^c holds because a / b^c < 1.
            let time_expr = if k.abs() < eps {
                if (c - 1.0).abs() < eps {
                    variable(var).unwrap()
                } else if c.abs() < eps {
                    constant(1.0).unwrap()
                } else {
                    power(variable(var).unwrap(), c).unwrap()
                }
            } else {
                let log_term = logarithm(variable(var).unwrap(), 2.0).unwrap();
                let log_factor = if (k - 1.0).abs() < eps {
                    log_term
                } else {
                    power(log_term, k).unwrap()
                };
                if (c - 1.0).abs() < eps {
                    multiply(vec![variable(var).unwrap(), log_factor])
                } else {
                    multiply(vec![power(variable(var).unwrap(), c).unwrap(), log_factor])
                }
            };

            let asymp = if k.abs() < eps {
                if (c - 1.0).abs() < eps {
                    "Theta(n)".to_string()
                } else {
                    format!("Theta(n^{})", format_exponent(c))
                }
            } else if (c - 1.0).abs() < eps {
                format!("Theta(n (log n)^{})", format_exponent(k))
            } else {
                format!(
                    "Theta(n^{} (log n)^{})",
                    format_exponent(c),
                    format_exponent(k)
                )
            };

            (time_expr, asymp)
        };

        Some(RecurrenceSolution {
            status: BoundStatus::Proven,
            time,
            asymptotic,
            method: self.name().to_string(),
            assumptions: vec!["base case assumed to require O(1) time".to_string()],
            limitations: Vec::new(),
        })
    }
}

/// Solves linear decrement recurrences:
/// T(n) = T(n - c) + Theta(n^k)
#[derive(Debug, Default, Clone, Copy)]
pub struct LinearDecrementSolver;

impl RecurrenceSolver for LinearDecrementSolver {
    fn name(&self) -> &'static str {
        "linear-decrement"
    }

    fn solve(&self, spec: &RecurrenceSpec) -> Option<RecurrenceSolution> {
        // If divide recurrence, linear solver does not apply
        if spec.b.is_some() && spec.c_subtrahend.is_none() {
            return None;
        }

        // Check if multiple subtractive recursion (Fibonacci)
        let decr_calls = spec
            .calls
            .iter()
            .filter(|c| c.change == SizeChange::Decrement)
            .count();
        let max_calls = spec.max_calls_per_invocation.unwrap_or(decr_calls);
        if max_calls > 1 || decr_calls > 1 {
            return Some(RecurrenceSolution {
                status: BoundStatus::Unsupported,
                time: Arc::new(Expr::Constant(0.0)),
                asymptotic: "unsupported".to_string(),
                method: self.name().to_string(),
                assumptions: Vec::new(),
                limitations: vec![
                    "multiple recursion with subtractive arguments is not supported; exponential bounds are not derived".to_string(),
                ],
            });
        }

        let c = match spec.c_subtrahend {
            Some(val) => val,
            None => {
                let call = spec
                    .calls
                    .iter()
                    .find(|call| call.change == SizeChange::Decrement)?;
                call.amount
            }
        };

        if !c.is_finite() || c <= 0.0 {
            return Some(RecurrenceSolution {
                status: BoundStatus::Unsupported,
                time: Arc::new(Expr::Constant(0.0)),
                asymptotic: "unsupported".to_string(),
                method: self.name().to_string(),
                assumptions: Vec::new(),
                limitations: vec![format!(
                    "linear decrement requires step c > 0; received c = {}",
                    c
                )],
            });
        }

        let var = &spec.variable;
        let k = spec.c_work;

        let (time, asymptotic) = if k.abs() < 1e-6 {
            // Constant work: \sum_{i=1}^{n/c} 1 = n/c => Theta(n)
            let time_expr = if (c - 1.0).abs() < 1e-6 {
                variable(var).unwrap()
            } else {
                multiply(vec![constant(1.0 / c).unwrap(), variable(var).unwrap()])
            };
            (time_expr, "Theta(n)".to_string())
        } else {
            // Polynomial work: \sum_{i=1}^{n/c} (i*c)^k => Theta(n^(k + 1))
            let result_exp = k + 1.0;
            let time_expr = power(variable(var).unwrap(), result_exp).unwrap();
            let asymp = format!("Theta(n^{})", format_exponent(result_exp));
            (time_expr, asymp)
        };

        Some(RecurrenceSolution {
            status: BoundStatus::Proven,
            time,
            asymptotic,
            method: self.name().to_string(),
            assumptions: vec!["base case assumed to require O(1) time".to_string()],
            limitations: Vec::new(),
        })
    }
}

/// Solves a recurrence specification by dispatching to standard solvers.
pub fn solve_recurrence(spec: &RecurrenceSpec) -> Option<RecurrenceSolution> {
    // Check linear decrement first if subtrahend is explicitly specified
    if spec.c_subtrahend.is_some() {
        let linear = LinearDecrementSolver;
        if let Some(sol) = linear.solve(spec) {
            return Some(sol);
        }
    }

    // Check divide-and-conquer if b is explicitly specified
    if spec.b.is_some() {
        let master = MasterTheoremSolver;
        if let Some(sol) = master.solve(spec) {
            return Some(sol);
        }
    }

    // Try Master Theorem
    let master = MasterTheoremSolver;
    if let Some(sol) = master.solve(spec) {
        return Some(sol);
    }

    // Try Linear Decrement
    let linear = LinearDecrementSolver;
    if let Some(sol) = linear.solve(spec) {
        return Some(sol);
    }

    None
}

fn round_exponent(val: f64) -> f64 {
    let nearest_int = val.round();
    if (val - nearest_int).abs() < 1e-6 {
        nearest_int
    } else {
        (val * 1000.0).round() / 1000.0
    }
}

fn format_exponent(val: f64) -> String {
    let rounded = round_exponent(val);
    if (rounded - rounded.round()).abs() < 1e-6 {
        format!("{}", rounded as i64)
    } else {
        format!("{:.3}", rounded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_sort_recurrence() {
        // Merge sort: T(n) = 2 * T(n / 2) + Theta(n) => Theta(n log n)
        let spec = RecurrenceSpec::divide_and_conquer(2.0, 2.0, 1.0, 0.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "master-theorem");
        assert!(sol.asymptotic.contains("n log n"));
        assert_eq!(sol.time.to_string(), "n * log_2(n)");
    }

    #[test]
    fn test_binary_search_recurrence() {
        // Binary search: T(n) = 1 * T(n / 2) + Theta(1) => Theta(log n)
        let spec = RecurrenceSpec::divide_and_conquer(1.0, 2.0, 0.0, 0.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "master-theorem");
        assert!(sol.asymptotic.contains("log n"));
        assert_eq!(sol.time.to_string(), "log_2(n)");
    }

    #[test]
    fn test_strassen_recurrence() {
        // Strassen: T(n) = 7 * T(n / 2) + Theta(n^2) => Theta(n^2.807)
        let spec = RecurrenceSpec::divide_and_conquer(7.0, 2.0, 2.0, 0.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "master-theorem");
        assert!(sol.asymptotic.contains("n^2.807"));
        assert_eq!(sol.time.to_string(), "n^2.807");
    }

    #[test]
    fn test_linear_decrement_constant_work() {
        // T(n) = T(n - 1) + 1 => Theta(n)
        let spec = RecurrenceSpec::linear_decrement(1.0, 0.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "linear-decrement");
        assert_eq!(sol.asymptotic, "Theta(n)");
        assert_eq!(sol.time.to_string(), "n");
    }

    #[test]
    fn test_linear_decrement_linear_work() {
        // T(n) = T(n - 1) + n => Theta(n^2)
        let spec = RecurrenceSpec::linear_decrement(1.0, 1.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "linear-decrement");
        assert_eq!(sol.asymptotic, "Theta(n^2)");
        assert_eq!(sol.time.to_string(), "n^2");
    }

    #[test]
    fn test_linear_decrement_step_two() {
        // T(n) = T(n - 2) + 1 => Theta(n), time = 0.5 * n
        let spec = RecurrenceSpec::linear_decrement(2.0, 0.0);
        let sol = solve_recurrence(&spec).expect("solution expected");
        assert_eq!(sol.status, BoundStatus::Proven);
        assert_eq!(sol.method, "linear-decrement");
        assert_eq!(sol.asymptotic, "Theta(n)");
        assert_eq!(sol.time.to_string(), "0.5 * n");
    }

    #[test]
    fn test_invalid_divisor_unsupported() {
        // b <= 1 is invalid for Master Theorem
        let spec = RecurrenceSpec::divide_and_conquer(2.0, 1.0, 1.0, 0.0);
        let sol = solve_recurrence(&spec).expect("handled unsupported recurrence");
        assert_eq!(sol.status, BoundStatus::Unsupported);
        assert!(sol.limitations[0].contains("b > 1.0"));
    }

    #[test]
    fn test_invalid_branch_factor_unsupported() {
        // a <= 0 is invalid for Master Theorem
        let spec = RecurrenceSpec::divide_and_conquer(0.0, 2.0, 1.0, 0.0);
        let sol = solve_recurrence(&spec).expect("handled unsupported recurrence");
        assert_eq!(sol.status, BoundStatus::Unsupported);
        assert!(sol.limitations[0].contains("a >= 1"));
    }

    #[test]
    fn test_multiple_subtractive_unsupported() {
        // Fibonacci: T(n) = T(n - 1) + T(n - 2) + 1 => Unsupported
        let spec = RecurrenceSpec::multiple_subtractive(vec![1.0, 2.0], 0.0);
        let sol = solve_recurrence(&spec).expect("handled unsupported recurrence");
        assert_eq!(sol.status, BoundStatus::Unsupported);
        assert!(sol.limitations[0].contains("multiple recursion with subtractive arguments"));
    }

    #[test]
    fn test_invalid_step_unsupported() {
        // c <= 0 is invalid for linear decrement
        let spec = RecurrenceSpec::linear_decrement(0.0, 0.0);
        let sol = solve_recurrence(&spec).expect("handled unsupported recurrence");
        assert_eq!(sol.status, BoundStatus::Unsupported);
        assert!(sol.limitations[0].contains("step c > 0"));
    }
}
