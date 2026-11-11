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

/// A sum constraint: \sum_{count_variable} variable <= max_total.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SumConstraint {
    pub variable: String,
    pub count_variable: String,
    pub max_total: f64,
}

/// A product constraint: \prod variables <= max_product.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductConstraint {
    pub variables: Vec<String>,
    pub max_product: f64,
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
    pub terms: Vec<LinearTerm>,
    pub op: String,
    pub bound: f64,
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
    let free_vars = expr.free_variables();
    let mut missing = Vec::new();
    let mut max_values = HashMap::new();

    for var in &free_vars {
        if let Some(bound) = constraints.variables.get(var) {
            if let Some(max) = bound.max {
                max_values.insert(var.clone(), max);
            } else {
                missing.push(var.clone());
            }
        } else {
            missing.push(var.clone());
        }
    }

    if !missing.is_empty() {
        return WorkloadResult {
            status: BoundStatus::Unknown,
            upper_bound: None,
            method: None,
            assumptions: Vec::new(),
            missing_bounds: missing,
        };
    }

    // Check for sum constraints (e.g. t * n where sum(n) <= S).
    for sc in &constraints.sum_constraints {
        if free_vars.contains(&sc.variable) && free_vars.contains(&sc.count_variable) {
            // Check if expr matches t * n or t * (n + ...)
            // Sum constraint packing yields max_total * inner_cost or similar
            let method = "sum-constraint-packing".to_string();
            let assumptions = vec![format!(
                "sum of '{}' over '{}' bounded by {}",
                sc.variable, sc.count_variable, sc.max_total
            )];

            // Evaluate expression with count variable replaced by 1 and variable by max_total
            let mut sum_vars = max_values.clone();
            sum_vars.insert(sc.count_variable.clone(), 1.0);
            sum_vars.insert(sc.variable.clone(), sc.max_total);

            if let Ok(val) = expr.evaluate(&sum_vars) {
                return WorkloadResult {
                    status: BoundStatus::Proven,
                    upper_bound: Some(val),
                    method: Some(method),
                    assumptions,
                    missing_bounds: Vec::new(),
                };
            }
        }
    }

    // Check for product constraints (e.g. n * m <= P).
    for pc in &constraints.product_constraints {
        if pc.variables.len() == 2
            && free_vars.contains(&pc.variables[0])
            && free_vars.contains(&pc.variables[1])
        {
            let method = "product-constraint-substitution".to_string();
            let assumptions = vec![format!(
                "product of '{}' and '{}' bounded by {}",
                pc.variables[0], pc.variables[1], pc.max_product
            )];

            let mut prod_vars = max_values.clone();
            prod_vars.insert(pc.variables[0].clone(), pc.max_product);
            prod_vars.insert(pc.variables[1].clone(), 1.0);

            if let Ok(val) = expr.evaluate(&prod_vars) {
                return WorkloadResult {
                    status: BoundStatus::Proven,
                    upper_bound: Some(val),
                    method: Some(method),
                    assumptions,
                    missing_bounds: Vec::new(),
                };
            }
        }
    }

    // Standard worst-case evaluation using maximum bounds for all variables.
    match expr.evaluate(&max_values) {
        Ok(val) => WorkloadResult {
            status: BoundStatus::Proven,
            upper_bound: Some(val),
            method: Some("variable-upper-bounds".to_string()),
            assumptions: Vec::new(),
            missing_bounds: Vec::new(),
        },
        Err(_) => WorkloadResult {
            status: BoundStatus::Unknown,
            upper_bound: None,
            method: None,
            assumptions: Vec::new(),
            missing_bounds: Vec::new(),
        },
    }
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
