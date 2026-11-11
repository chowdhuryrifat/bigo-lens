//! Constraint model and worst-case workload evaluation.

use crate::complexity::Expr;
use crate::models::BoundStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Upper and lower bounds for a single named variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VariableBound {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl VariableBound {
    pub fn new(min: Option<f64>, max: Option<f64>) -> Self {
        Self { min, max }
    }

    pub fn range(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    pub fn max_only(max: f64) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }
}

/// A sum constraint: \sum_{count_variable} variable <= max_total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SumConstraint {
    pub variable: String,
    pub count_variable: String,
    pub max_total: f64,
}

impl SumConstraint {
    pub fn new(
        variable: impl Into<String>,
        count_variable: impl Into<String>,
        max_total: f64,
    ) -> Self {
        Self {
            variable: variable.into(),
            count_variable: count_variable.into(),
            max_total,
        }
    }
}

/// A product constraint: \prod variables <= max_product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductConstraint {
    pub variables: Vec<String>,
    pub max_product: f64,
}

impl ProductConstraint {
    pub fn new(variables: Vec<impl Into<String>>, max_product: f64) -> Self {
        Self {
            variables: variables.into_iter().map(Into::into).collect(),
            max_product,
        }
    }
}

/// A term in a linear constraint: coeff * variable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearTerm {
    pub coefficient: f64,
    pub variable: String,
}

/// A linear constraint: \sum coeff_i * var_i <= bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinearConstraint {
    #[serde(default)]
    pub coefficients: HashMap<String, f64>,
    #[serde(default)]
    pub terms: Vec<LinearTerm>,
    #[serde(default = "default_linear_op")]
    pub op: String,
    pub bound: f64,
}

fn default_linear_op() -> String {
    "<=".to_string()
}

impl LinearConstraint {
    pub fn new(coefficients: HashMap<String, f64>, bound: f64) -> Self {
        Self {
            coefficients,
            terms: Vec::new(),
            op: "<=".to_string(),
            bound,
        }
    }

    pub fn from_terms(terms: Vec<LinearTerm>, bound: f64) -> Self {
        let mut coefficients = HashMap::new();
        for t in &terms {
            *coefficients.entry(t.variable.clone()).or_insert(0.0) += t.coefficient;
        }
        Self {
            coefficients,
            terms,
            op: "<=".to_string(),
            bound,
        }
    }

    pub fn get_coefficients(&self) -> HashMap<String, f64> {
        let mut map = self.coefficients.clone();
        for term in &self.terms {
            *map.entry(term.variable.clone()).or_insert(0.0) += term.coefficient;
        }
        map
    }
}

/// Complete set of constraints for a solution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConstraintSet {
    #[serde(default)]
    pub variables: HashMap<String, VariableBound>,
    #[serde(default)]
    pub sum_constraints: Vec<SumConstraint>,
    #[serde(default)]
    pub product_constraints: Vec<ProductConstraint>,
    #[serde(default)]
    pub linear_constraints: Vec<LinearConstraint>,
    pub time_limit_seconds: Option<f64>,
    pub memory_limit_mb: Option<f64>,
}

/// Result of evaluating an expression under constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkloadResult {
    pub status: BoundStatus,
    pub upper_bound: Option<f64>,
    pub method: Option<String>,
    pub assumptions: Vec<String>,
    pub missing_bounds: Vec<String>,
}

/// Evaluate worst-case upper bound of an expression under the given constraints.
pub fn evaluate_workload(expr: &Expr, constraints: &ConstraintSet) -> WorkloadResult {
    crate::solvers::evaluate_workload_tightened(expr, constraints)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::variable;

    #[test]
    fn test_variable_upper_bounds() {
        let n = variable("n").unwrap();
        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(1000.0),
            },
        );

        let res = evaluate_workload(&n, &constraints);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(1000.0));
    }
}
