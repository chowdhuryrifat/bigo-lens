//! Advanced constraint tightening algorithms for workload evaluation.
//!
//! Conforms strictly to ADR 0012, CONSTRAINT_MODEL.md (§5, §6, §9), and RULES.md.

use crate::complexity::{ComplexityError, Expr, ExprPtr};
use crate::constraints::{
    ConstraintSet, LinearConstraint, ProductConstraint, SumConstraint, WorkloadResult,
};
use crate::models::BoundStatus;
use std::collections::HashMap;
use std::sync::Arc;

/// Context tracking constraints and their application during expression evaluation.
#[derive(Debug, Clone)]
pub struct ConstraintContext<'a> {
    pub products: &'a [ProductConstraint],
    pub linear: &'a [LinearConstraint],
    pub minima: &'a HashMap<String, f64>,
    pub product_applied: Vec<bool>,
    pub product_tightened: Vec<bool>,
    pub linear_applied: Vec<bool>,
    pub linear_tightened: Vec<bool>,
    pub remaining_factors_at_maxima: bool,
}

impl<'a> ConstraintContext<'a> {
    pub fn new(
        products: &'a [ProductConstraint],
        linear: &'a [LinearConstraint],
        minima: &'a HashMap<String, f64>,
    ) -> Self {
        Self {
            products,
            linear,
            minima,
            product_applied: vec![false; products.len()],
            product_tightened: vec![false; products.len()],
            linear_applied: vec![false; linear.len()],
            linear_tightened: vec![false; linear.len()],
            remaining_factors_at_maxima: false,
        }
    }
}

/// Check if an expression is syntactically monotone non-decreasing (assuming variables >= 0).
pub fn is_monotone_non_decreasing(expr: &Expr) -> bool {
    match expr {
        Expr::Constant(_) | Expr::Variable(_) => true,
        Expr::Add(ops) | Expr::Maximum(ops) => ops.iter().all(|op| is_monotone_non_decreasing(op)),
        Expr::Multiply(ops) => ops.iter().all(|op| is_monotone_non_decreasing(op)),
        Expr::Power { base, exponent } => *exponent >= 0.0 && is_monotone_non_decreasing(base),
        Expr::Logarithm { arg, base } => *base > 1.0 && is_monotone_non_decreasing(arg),
        Expr::Summation {
            lower, upper, body, ..
        } => {
            lower.free_variables().is_empty()
                && is_monotone_non_decreasing(upper)
                && is_monotone_non_decreasing(body)
        }
    }
}

/// Check if an expression is superadditive in a specific variable: f(a) + f(b) <= f(a + b).
pub fn is_superadditive_in(expr: &Expr, v: &str) -> bool {
    match expr {
        Expr::Constant(c) => *c <= 0.0,
        Expr::Variable(name) => name == v,
        Expr::Add(ops) => ops.iter().all(|op| is_superadditive_in(op, v)),
        Expr::Multiply(ops) => {
            let mut has_super = false;
            for op in ops {
                if is_superadditive_in(op, v) {
                    has_super = true;
                } else if !is_monotone_non_decreasing(op) {
                    return false;
                }
            }
            has_super
        }
        Expr::Power { base, exponent } => *exponent >= 1.0 && is_superadditive_in(base, v),
        Expr::Logarithm { .. } => false,
        Expr::Maximum(ops) => ops.iter().any(|op| is_superadditive_in(op, v)),
        Expr::Summation {
            lower, upper, body, ..
        } => {
            lower.free_variables().is_empty()
                && is_superadditive_in(upper, v)
                && is_monotone_non_decreasing(body)
        }
    }
}

/// Try to factor out a single count variable factor from an expression (e.g. t * per_case -> per_case).
pub fn strip_count_factor(expr: &Expr, count: &str) -> Option<ExprPtr> {
    match expr {
        Expr::Variable(name) if name == count => crate::complexity::constant(1.0).ok(),
        Expr::Multiply(factors) => {
            let mut occurrences = 0;
            let mut rest = Vec::new();

            for factor in factors {
                let is_count = match factor.as_ref() {
                    Expr::Variable(name) => name == count,
                    Expr::Maximum(ops) if ops.len() == 2 => {
                        let c0 = matches!(ops[0].as_ref(), Expr::Constant(c) if *c == 0.0);
                        let v = matches!(ops[1].as_ref(), Expr::Variable(name) if name == count);
                        c0 && v
                    }
                    _ => false,
                };

                if is_count {
                    occurrences += 1;
                } else {
                    rest.push(Arc::clone(factor));
                }
            }

            if occurrences != 1 {
                return None;
            }

            let prod = crate::complexity::multiply(rest);
            if prod.free_variables().contains(count) {
                None
            } else {
                Some(prod)
            }
        }
        _ => None,
    }
}

/// Tighten individual variable upper bounds iteratively using linear constraints.
pub fn tighten_variable_bounds_with_linear(
    maxima: &mut HashMap<String, f64>,
    minima: &HashMap<String, f64>,
    linear: &[LinearConstraint],
    linear_applied: &mut [bool],
    linear_tightened: &mut [bool],
) {
    let mut changed = true;
    for _ in 0..5 {
        if !changed {
            break;
        }
        changed = false;

        for (i, lc) in linear.iter().enumerate() {
            let coeffs = lc.get_coefficients();
            if coeffs.values().any(|&c| c < 0.0) {
                continue;
            }

            for (v, &coeff) in &coeffs {
                if coeff <= 0.0 {
                    continue;
                }
                let current_max = match maxima.get(v) {
                    Some(&m) => m,
                    None => continue,
                };

                let mut min_other = 0.0;
                let mut other_ok = true;
                for (u, &ucoeff) in &coeffs {
                    if u == v || ucoeff <= 0.0 {
                        continue;
                    }
                    match minima.get(u) {
                        Some(&mu) if mu >= 0.0 => {
                            min_other += ucoeff * mu;
                        }
                        _ => {
                            other_ok = false;
                            break;
                        }
                    }
                }
                if !other_ok {
                    continue;
                }

                let remaining = lc.bound - min_other;
                if remaining >= 0.0 {
                    let cap = (remaining / coeff).floor();
                    if cap < current_max {
                        maxima.insert(v.clone(), cap);
                        linear_applied[i] = true;
                        linear_tightened[i] = true;
                        changed = true;
                    }
                }
            }
        }
    }
}

/// Flatten nested multiplications and extract variable powers and other factors.
fn extract_factors(expr: &Expr, vars: &mut HashMap<String, f64>, others: &mut Vec<ExprPtr>) {
    match expr {
        Expr::Multiply(factors) => {
            for f in factors {
                extract_factors(f, vars, others);
            }
        }
        Expr::Variable(name) => {
            *vars.entry(name.clone()).or_insert(0.0) += 1.0;
        }
        Expr::Power { base, exponent } => {
            if let Expr::Variable(name) = base.as_ref() {
                if *exponent > 0.0 {
                    *vars.entry(name.clone()).or_insert(0.0) += *exponent;
                    return;
                }
            }
            others.push(Arc::new(expr.clone()));
        }
        _ => {
            others.push(Arc::new(expr.clone()));
        }
    }
}

/// Evaluates a monotone expression with product and linear constraint tightening.
pub fn eval_monotone_with_constraints(
    expr: &Expr,
    bindings: &HashMap<String, f64>,
    ctx: &mut ConstraintContext,
) -> Result<f64, ComplexityError> {
    match expr {
        Expr::Constant(c) => Ok(*c),
        Expr::Variable(name) => bindings
            .get(name)
            .copied()
            .ok_or_else(|| ComplexityError::UnboundVariable(name.clone())),
        Expr::Maximum(ops) => {
            let mut max_val = f64::NEG_INFINITY;
            for op in ops {
                let v = eval_monotone_with_constraints(op, bindings, ctx)?;
                if v > max_val {
                    max_val = v;
                }
            }
            Ok(max_val)
        }
        Expr::Power { base, exponent } => {
            let b = eval_monotone_with_constraints(base, bindings, ctx)?;
            let v = b.powf(*exponent);
            if !v.is_finite() {
                Err(ComplexityError::NonFinite)
            } else {
                Ok(v)
            }
        }
        Expr::Logarithm { arg, base } => {
            let a = eval_monotone_with_constraints(arg, bindings, ctx)?.max(1.0);
            let v = a.log(*base);
            if !v.is_finite() {
                Err(ComplexityError::NonFinite)
            } else {
                Ok(v)
            }
        }
        Expr::Summation { .. } => expr.evaluate(bindings),
        Expr::Add(ops) => {
            let mut acc = 0.0;
            for op in ops {
                acc += eval_monotone_with_constraints(op, bindings, ctx)?;
            }

            // Check if linear constraint tightens the sum \sum \alpha_i x_i
            let mut lin_terms: HashMap<String, f64> = HashMap::new();
            let mut non_lin_sum = 0.0;

            for term in ops {
                match term.as_ref() {
                    Expr::Variable(name) => {
                        *lin_terms.entry(name.clone()).or_insert(0.0) += 1.0;
                    }
                    Expr::Multiply(factors) if factors.len() == 2 => {
                        let (f0, f1) = (&factors[0], &factors[1]);
                        if let (Expr::Constant(c), Expr::Variable(name)) =
                            (f0.as_ref(), f1.as_ref())
                        {
                            if *c > 0.0 {
                                *lin_terms.entry(name.clone()).or_insert(0.0) += *c;
                                continue;
                            }
                        }
                        if let (Expr::Variable(name), Expr::Constant(c)) =
                            (f0.as_ref(), f1.as_ref())
                        {
                            if *c > 0.0 {
                                *lin_terms.entry(name.clone()).or_insert(0.0) += *c;
                                continue;
                            }
                        }
                        non_lin_sum += eval_monotone_with_constraints(term, bindings, ctx)?;
                    }
                    _ => {
                        non_lin_sum += eval_monotone_with_constraints(term, bindings, ctx)?;
                    }
                }
            }

            if lin_terms.len() >= 2 {
                for (i, lc) in ctx.linear.iter().enumerate() {
                    let coeffs = lc.get_coefficients();
                    if coeffs.values().any(|&c| c < 0.0) {
                        continue;
                    }

                    let mut covers_all = true;
                    let mut max_ratio = 0.0f64;

                    for (name, alpha) in &lin_terms {
                        match coeffs.get(name) {
                            Some(&c) if c > 0.0 => {
                                let ratio = alpha / c;
                                if ratio > max_ratio {
                                    max_ratio = ratio;
                                }
                            }
                            _ => {
                                covers_all = false;
                                break;
                            }
                        }
                    }

                    if !covers_all {
                        continue;
                    }

                    let mut min_other = 0.0;
                    for (u, &ucoeff) in &coeffs {
                        if !lin_terms.contains_key(u) && ucoeff > 0.0 {
                            if let Some(&mu) = ctx.minima.get(u) {
                                min_other += ucoeff * mu;
                            }
                        }
                    }

                    let beff = (lc.bound - min_other).max(0.0);
                    let lin_sum_bound = max_ratio * beff + non_lin_sum;
                    if lin_sum_bound < acc {
                        acc = lin_sum_bound;
                        ctx.linear_applied[i] = true;
                        ctx.linear_tightened[i] = true;
                    }
                }
            }

            Ok(acc)
        }
        Expr::Multiply(_) => {
            let mut var_exponents = HashMap::new();
            let mut other_factors = Vec::new();
            extract_factors(expr, &mut var_exponents, &mut other_factors);

            let mut acc = 1.0;

            loop {
                #[derive(Debug, PartialEq)]
                enum CandidateType {
                    None,
                    Product,
                    Linear,
                }

                let mut best_type = CandidateType::None;
                let mut best_idx = 0;
                let mut best_reduction = 1.0;

                let mut best_prod_amin = 0.0;
                let mut best_prod_peff = 0.0;
                let mut best_prod_mprod = 0.0;

                let mut best_lin_bound = 0.0;
                let mut best_lin_vars = Vec::new();

                // 1. Check Product Constraints
                for (i, pc) in ctx.products.iter().enumerate() {
                    let mut all_present = true;
                    let mut amin = f64::INFINITY;
                    let mut mprod = 1.0;

                    for v in &pc.variables {
                        match var_exponents.get(v) {
                            Some(&exp) if exp >= 1.0 => {
                                if exp < amin {
                                    amin = exp;
                                }
                            }
                            _ => {
                                all_present = false;
                                break;
                            }
                        }

                        match bindings.get(v) {
                            Some(&mv) => mprod *= mv,
                            None => {
                                all_present = false;
                                break;
                            }
                        }
                    }

                    if !all_present || amin < 1.0 {
                        continue;
                    }

                    let peff = pc.max_product.min(mprod);
                    let reduction = if mprod > 0.0 {
                        (peff / mprod).powf(amin)
                    } else {
                        1.0
                    };

                    if reduction < best_reduction
                        || (best_type == CandidateType::None && peff <= mprod)
                    {
                        best_reduction = reduction;
                        best_type = CandidateType::Product;
                        best_idx = i;
                        best_prod_amin = amin;
                        best_prod_peff = peff;
                        best_prod_mprod = mprod;
                    }
                }

                // 2. Check Linear Constraints
                for (j, lc) in ctx.linear.iter().enumerate() {
                    let coeffs = lc.get_coefficients();
                    if coeffs.values().any(|&c| c < 0.0) {
                        continue;
                    }

                    let mut matched_vars = Vec::new();
                    for (v, &coeff) in &coeffs {
                        if coeff > 0.0 {
                            if let Some(&exp) = var_exponents.get(v) {
                                if exp >= 1.0 {
                                    matched_vars.push(v.clone());
                                }
                            }
                        }
                    }

                    if matched_vars.len() == 2 {
                        let x = &matched_vars[0];
                        let y = &matched_vars[1];
                        let cx = coeffs[x];
                        let cy = coeffs[y];
                        let ax = var_exponents[x];
                        let ay = var_exponents[y];
                        let mx = ctx.minima.get(x).copied().unwrap_or(0.0);
                        let my = ctx.minima.get(y).copied().unwrap_or(0.0);
                        let mx_max = bindings[x];
                        let my_max = bindings[y];

                        let mut min_other = 0.0;
                        for (u, &ucoeff) in &coeffs {
                            if u != x && u != y && ucoeff > 0.0 {
                                min_other += ucoeff * ctx.minima.get(u).copied().unwrap_or(0.0);
                            }
                        }

                        let beff = lc.bound - min_other;
                        if beff > 0.0 {
                            let xstar = (ax / (ax + ay)) * (beff / cx);
                            let xlo = mx.max((beff - cy * my_max) / cx);
                            let xhi = mx_max.min((beff - cy * my) / cx);

                            if xlo <= xhi {
                                let xopt = xstar.clamp(xlo, xhi);
                                let yopt = (0.0f64).max((beff - cx * xopt) / cy);
                                let lin_bound = xopt.powf(ax) * yopt.powf(ay);
                                let mprod = mx_max.powf(ax) * my_max.powf(ay);
                                let reduction = if mprod > 0.0 { lin_bound / mprod } else { 1.0 };

                                if reduction < best_reduction
                                    || (best_type == CandidateType::None && lin_bound <= mprod)
                                {
                                    best_reduction = reduction;
                                    best_type = CandidateType::Linear;
                                    best_idx = j;
                                    best_lin_bound = lin_bound;
                                    best_lin_vars = matched_vars;
                                }
                            }
                        }
                    } else if matched_vars.len() > 2 {
                        let sum_a: f64 = matched_vars.iter().map(|v| var_exponents[v]).sum();
                        let mut min_other = 0.0;
                        for (u, &ucoeff) in &coeffs {
                            if ucoeff > 0.0 && !matched_vars.contains(u) {
                                min_other += ucoeff * ctx.minima.get(u).copied().unwrap_or(0.0);
                            }
                        }

                        let beff = lc.bound - min_other;
                        if beff > 0.0 && sum_a > 0.0 {
                            let mut lin_bound = 1.0;
                            let mut mprod = 1.0;
                            for v in &matched_vars {
                                let cv = coeffs[v];
                                let av = var_exponents[v];
                                let mv = ctx.minima.get(v).copied().unwrap_or(0.0);
                                let mv_max = bindings[v];
                                let xvstar = (av / sum_a) * (beff / cv);
                                let xvopt = xvstar.clamp(mv, mv_max);
                                lin_bound *= xvopt.powf(av);
                                mprod *= mv_max.powf(av);
                            }

                            let reduction = if mprod > 0.0 { lin_bound / mprod } else { 1.0 };
                            if reduction < best_reduction
                                || (best_type == CandidateType::None && lin_bound <= mprod)
                            {
                                best_reduction = reduction;
                                best_type = CandidateType::Linear;
                                best_idx = j;
                                best_lin_bound = lin_bound;
                                best_lin_vars = matched_vars;
                            }
                        }
                    }
                }

                if best_type == CandidateType::None {
                    break;
                }

                if best_type == CandidateType::Product {
                    let pc = &ctx.products[best_idx];
                    ctx.product_applied[best_idx] = true;
                    if best_prod_peff < best_prod_mprod {
                        ctx.product_tightened[best_idx] = true;
                    }
                    acc *= best_prod_peff.powf(best_prod_amin);
                    for v in &pc.variables {
                        if var_exponents[v] > best_prod_amin {
                            ctx.remaining_factors_at_maxima = true;
                        }
                        if let Some(exp) = var_exponents.get_mut(v) {
                            *exp -= best_prod_amin;
                        }
                    }
                } else if best_type == CandidateType::Linear {
                    ctx.linear_applied[best_idx] = true;
                    ctx.linear_tightened[best_idx] = true;
                    acc *= best_lin_bound;
                    for v in &best_lin_vars {
                        var_exponents.insert(v.clone(), 0.0);
                    }
                }
            }

            // Remaining variable powers
            for (name, exp) in &var_exponents {
                if *exp > 0.0 {
                    let mv = bindings
                        .get(name)
                        .copied()
                        .ok_or_else(|| ComplexityError::UnboundVariable(name.clone()))?;
                    acc *= mv.powf(*exp);
                }
            }

            // Other factors
            for of in &other_factors {
                acc *= eval_monotone_with_constraints(of, bindings, ctx)?;
            }

            if !acc.is_finite() {
                Err(ComplexityError::NonFinite)
            } else {
                Ok(acc)
            }
        }
    }
}

/// Helper to decompose an expression into additive terms.
fn collect_additive_terms(expr: &Expr, terms: &mut Vec<ExprPtr>) {
    if let Expr::Add(ops) = expr {
        for op in ops {
            collect_additive_terms(op, terms);
        }
    } else {
        terms.push(Arc::new(expr.clone()));
    }
}

/// Advanced workload evaluation tightening under product, linear, and separable sum constraints.
pub fn evaluate_workload_tightened(expr: &Expr, constraints: &ConstraintSet) -> WorkloadResult {
    let free = expr.free_variables();
    let mut missing = Vec::new();
    let mut maxima = HashMap::new();
    let mut minima = HashMap::new();
    let mut negative_allowed = false;

    for name in &free {
        if let Some(bound) = constraints.variables.get(name) {
            if let Some(max) = bound.max {
                maxima.insert(name.clone(), max);
            } else {
                missing.push(name.clone());
            }
            let min_val = bound.min.unwrap_or(0.0);
            if min_val < 0.0 {
                negative_allowed = true;
            }
            minima.insert(name.clone(), min_val.max(0.0));
        } else {
            missing.push(name.clone());
        }
    }

    for (name, bound) in &constraints.variables {
        minima
            .entry(name.clone())
            .or_insert_with(|| bound.min.unwrap_or(0.0).max(0.0));
    }

    if !missing.is_empty() {
        let assumptions = missing
            .iter()
            .map(|name| format!("no upper bound configured for '{}'", name))
            .collect();
        return WorkloadResult {
            status: BoundStatus::Unknown,
            upper_bound: None,
            method: None,
            assumptions,
            missing_bounds: missing,
        };
    }

    let monotone = is_monotone_non_decreasing(expr) && !negative_allowed;
    let status = if monotone {
        BoundStatus::Proven
    } else {
        BoundStatus::Estimated
    };

    let mut assumptions = Vec::new();
    if !monotone {
        assumptions.push(
            "expression is not provably non-decreasing; evaluated at variable maxima, which may not be the worst case"
                .to_string(),
        );
    }
    for name in &free {
        if let Some(bound) = constraints.variables.get(name) {
            if bound.min.is_none() {
                assumptions.push(format!(
                    "'{}' assumed non-negative (no lower bound configured)",
                    name
                ));
            }
        }
    }

    // Find sum constraints matching free variables
    let mut sums = Vec::new();
    for sc in &constraints.sum_constraints {
        if free.contains(&sc.variable) {
            sums.push(sc);
        }
    }

    let mut ctx = ConstraintContext::new(
        &constraints.product_constraints,
        &constraints.linear_constraints,
        &minima,
    );

    // CASE 1: No sum constraints
    if sums.is_empty() {
        if monotone {
            tighten_variable_bounds_with_linear(
                &mut maxima,
                &minima,
                &constraints.linear_constraints,
                &mut ctx.linear_applied,
                &mut ctx.linear_tightened,
            );

            let eval_res = eval_monotone_with_constraints(expr, &maxima, &mut ctx);
            let val = match eval_res {
                Ok(v) => v,
                Err(e) => {
                    return WorkloadResult {
                        status: BoundStatus::Unknown,
                        upper_bound: None,
                        method: None,
                        assumptions: vec![format!("evaluation failed: {}", e)],
                        missing_bounds: Vec::new(),
                    };
                }
            };

            let any_prod_tightened = ctx.product_tightened.iter().any(|&t| t);
            let any_lin_tightened = ctx.linear_tightened.iter().any(|&t| t);

            // Assumptions for unapplied product constraints
            for (i, pc) in constraints.product_constraints.iter().enumerate() {
                let covered = pc.variables.iter().filter(|v| free.contains(*v)).count();
                if covered >= 2 && !ctx.product_applied[i] {
                    assumptions.push(format!(
                        "product constraint (bound {}) not used to tighten the bound; result may be loose",
                        pc.max_product as i64
                    ));
                }
            }

            // Assumptions for unapplied linear constraints
            for (i, lc) in constraints.linear_constraints.iter().enumerate() {
                let coeffs = lc.get_coefficients();
                if coeffs.values().any(|&c| c < 0.0) {
                    assumptions.push(format!(
                        "linear constraint with negative coefficient (bound {}) not used to tighten the bound; result may be loose",
                        lc.bound as i64
                    ));
                    continue;
                }
                let covered = coeffs.keys().filter(|v| free.contains(*v)).count();
                if covered >= 2 && !ctx.linear_applied[i] {
                    assumptions.push(format!(
                        "linear constraint (bound {}) not used to tighten the bound; result may be loose",
                        lc.bound as i64
                    ));
                }
            }

            let method = if any_prod_tightened {
                if ctx.remaining_factors_at_maxima {
                    assumptions.push(
                        "workload bounded by product constraint with remaining factors evaluated at individual maxima"
                            .to_string(),
                    );
                }
                "product-constraint-bound".to_string()
            } else if any_lin_tightened {
                "linear-constraint-bound".to_string()
            } else {
                "pointwise-maxima".to_string()
            };

            return WorkloadResult {
                status,
                upper_bound: Some(val),
                method: Some(method),
                assumptions,
                missing_bounds: Vec::new(),
            };
        } else {
            // Non-monotone fallback
            for pc in &constraints.product_constraints {
                let covered = pc.variables.iter().filter(|v| free.contains(*v)).count();
                if covered >= 2 {
                    assumptions.push(format!(
                        "product constraint (bound {}) not used to tighten the bound; result may be loose",
                        pc.max_product as i64
                    ));
                }
            }
            for lc in &constraints.linear_constraints {
                let coeffs = lc.get_coefficients();
                let covered = coeffs.keys().filter(|v| free.contains(*v)).count();
                if covered >= 2 {
                    assumptions.push(format!(
                        "linear constraint (bound {}) not used to tighten the bound on non-monotone expression; result may be loose",
                        lc.bound as i64
                    ));
                }
            }

            return match expr.evaluate(&maxima) {
                Ok(val) => WorkloadResult {
                    status: BoundStatus::Estimated,
                    upper_bound: Some(val),
                    method: Some("pointwise-maxima".to_string()),
                    assumptions,
                    missing_bounds: Vec::new(),
                },
                Err(e) => WorkloadResult {
                    status: BoundStatus::Unknown,
                    upper_bound: None,
                    method: None,
                    assumptions: vec![format!("evaluation failed: {}", e)],
                    missing_bounds: Vec::new(),
                },
            };
        }
    }

    // CASE 2: Sum constraints present
    let count_var = &sums[0].count_variable;
    let same_count = sums.iter().all(|sc| &sc.count_variable == count_var);
    if !same_count {
        return WorkloadResult {
            status: BoundStatus::Unknown,
            upper_bound: None,
            method: None,
            assumptions: vec![
                "sum constraints over different test-case counts are not supported".to_string(),
            ],
            missing_bounds: Vec::new(),
        };
    }

    let max_cases = constraints
        .variables
        .get(count_var)
        .and_then(|b| b.max)
        .unwrap_or(1.0);

    let mut capped = maxima.clone();
    for sc in &sums {
        if let Some(mv) = capped.get_mut(&sc.variable) {
            *mv = mv.min(sc.max_total);
        }
    }

    let per_case: ExprPtr;
    if free.contains(count_var) {
        if let Some(stripped) = strip_count_factor(expr, count_var) {
            per_case = stripped;
        } else {
            capped.insert(count_var.clone(), max_cases);
            assumptions.push(format!(
                "sum constraint over '{}' not fully exploited; each case evaluated at its individual maximum",
                count_var
            ));
            return match expr.evaluate(&capped) {
                Ok(val) => WorkloadResult {
                    status,
                    upper_bound: Some(val),
                    method: Some("pointwise-maxima-capped".to_string()),
                    assumptions,
                    missing_bounds: Vec::new(),
                },
                Err(e) => WorkloadResult {
                    status: BoundStatus::Unknown,
                    upper_bound: None,
                    method: None,
                    assumptions: vec![format!("evaluation failed: {}", e)],
                    missing_bounds: Vec::new(),
                },
            };
        }
    } else {
        per_case = Arc::new(expr.clone());
        assumptions.push(format!(
            "cost treated as per test case because '{}' is constrained across test cases ('{}')",
            sums[0].variable, count_var
        ));
    }

    // Single sum constraint packing
    if sums.len() == 1 && monotone && is_superadditive_in(&per_case, &sums[0].variable) {
        let sc = sums[0];
        let total = sc.max_total;
        let per_case_max = capped[&sc.variable];
        let full_cases = if per_case_max > 0.0 {
            (total / per_case_max).floor().min(max_cases)
        } else {
            0.0
        };
        let remainder = total - full_cases * per_case_max;

        let full_eval = eval_monotone_with_constraints(&per_case, &capped, &mut ctx);
        let full_val = match full_eval {
            Ok(v) => v,
            Err(e) => {
                return WorkloadResult {
                    status: BoundStatus::Unknown,
                    upper_bound: None,
                    method: None,
                    assumptions: vec![format!("evaluation failed: {}", e)],
                    missing_bounds: Vec::new(),
                };
            }
        };

        let mut total_val = full_cases * full_val;
        if full_cases < max_cases && remainder > 0.0 {
            let mut rest_bindings = capped.clone();
            rest_bindings.insert(sc.variable.clone(), remainder);
            if let Ok(part_val) =
                eval_monotone_with_constraints(&per_case, &rest_bindings, &mut ctx)
            {
                total_val += part_val;
            }
        }

        assumptions.push(format!(
            "worst case packs {} test case(s) of maximum size (superadditive per-case cost)",
            full_cases as i64
        ));

        return WorkloadResult {
            status: BoundStatus::Proven,
            upper_bound: Some(total_val),
            method: Some("sum-constraint-packing".to_string()),
            assumptions,
            missing_bounds: Vec::new(),
        };
    }

    // Multiple separable sum constraints
    if sums.len() > 1 && monotone {
        let mut terms = Vec::new();
        collect_additive_terms(&per_case, &mut terms);

        let sum_by_var: HashMap<String, &SumConstraint> =
            sums.iter().map(|sc| (sc.variable.clone(), *sc)).collect();

        let mut all_separable = true;
        for term in &terms {
            let term_free = term.free_variables();
            let sum_vars_count = term_free
                .iter()
                .filter(|v| sum_by_var.contains_key(*v))
                .count();
            if sum_vars_count > 1 {
                all_separable = false;
                break;
            }
        }

        if all_separable {
            let mut total_value = 0.0;
            let mut eval_ok = true;
            let mut any_packed = false;

            for term in &terms {
                let term_free = term.free_variables();
                let sc_opt = term_free.iter().find_map(|v| sum_by_var.get(v).copied());

                match sc_opt {
                    None => match eval_monotone_with_constraints(term, &capped, &mut ctx) {
                        Ok(v) => total_value += max_cases * v,
                        Err(_) => {
                            eval_ok = false;
                            break;
                        }
                    },
                    Some(sc) => {
                        let total = sc.max_total;
                        let per_case_max = capped[&sc.variable];

                        if is_superadditive_in(term, &sc.variable) {
                            any_packed = true;
                            let full_cases = if per_case_max > 0.0 {
                                (total / per_case_max).floor().min(max_cases)
                            } else {
                                0.0
                            };
                            let remainder = total - full_cases * per_case_max;

                            let full_val =
                                match eval_monotone_with_constraints(term, &capped, &mut ctx) {
                                    Ok(v) => v,
                                    Err(_) => {
                                        eval_ok = false;
                                        break;
                                    }
                                };

                            let mut term_val = full_cases * full_val;
                            if full_cases < max_cases && remainder > 0.0 {
                                let mut rest_bindings = capped.clone();
                                rest_bindings.insert(sc.variable.clone(), remainder);
                                if let Ok(pv) =
                                    eval_monotone_with_constraints(term, &rest_bindings, &mut ctx)
                                {
                                    term_val += pv;
                                }
                            }
                            total_value += term_val;
                        } else {
                            let mut term_cases = max_cases;
                            if let Some(bound) = constraints.variables.get(&sc.variable) {
                                if let Some(min) = bound.min {
                                    if min >= 1.0 {
                                        term_cases = term_cases.min((total / min).floor());
                                    }
                                }
                            }
                            match eval_monotone_with_constraints(term, &capped, &mut ctx) {
                                Ok(v) => total_value += term_cases * v,
                                Err(_) => {
                                    eval_ok = false;
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            if eval_ok {
                let method = if any_packed {
                    assumptions.push(
                        "worst case packs test case(s) of maximum size for sum-constrained variables (separable per-case cost)"
                            .to_string(),
                    );
                    "sum-constraint-packing".to_string()
                } else {
                    assumptions.push(
                        "per-case cost not recognised as superadditive; bound is cases x per-case maximum"
                            .to_string(),
                    );
                    "sum-constraint-cases".to_string()
                };

                return WorkloadResult {
                    status: BoundStatus::Proven,
                    upper_bound: Some(total_value),
                    method: Some(method),
                    assumptions,
                    missing_bounds: Vec::new(),
                };
            }
        }
    }

    // Sound fallback: cases x per_case
    let mut cases = max_cases;
    for sc in &sums {
        if let Some(bound) = constraints.variables.get(&sc.variable) {
            if let Some(min) = bound.min {
                if min >= 1.0 {
                    cases = cases.min((sc.max_total / min).floor());
                }
            }
        }
    }

    let per_eval = eval_monotone_with_constraints(&per_case, &capped, &mut ctx);
    match per_eval {
        Ok(per_val) => {
            assumptions.push(
                "per-case cost not recognised as superadditive; bound is cases x per-case maximum"
                    .to_string(),
            );
            WorkloadResult {
                status,
                upper_bound: Some(cases * per_val),
                method: Some("sum-constraint-cases".to_string()),
                assumptions,
                missing_bounds: Vec::new(),
            }
        }
        Err(e) => WorkloadResult {
            status: BoundStatus::Unknown,
            upper_bound: None,
            method: None,
            assumptions: vec![format!("evaluation failed: {}", e)],
            missing_bounds: Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::complexity::{add, constant, logarithm, multiply, power, variable};
    use crate::constraints::{LinearConstraint, ProductConstraint, SumConstraint, VariableBound};

    #[test]
    fn test_exact_product_constraint_tightens() {
        // n * m with n <= 1e6, m <= 1e6, n * m <= 1e6 => 1e6, product-constraint-bound, Proven
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let expr = multiply(vec![n, m]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 1e6));
        cs.product_constraints.push(ProductConstraint::new(
            vec!["n".to_string(), "m".to_string()],
            1e6,
        ));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(1e6));
        assert_eq!(res.method, Some("product-constraint-bound".to_string()));
    }

    #[test]
    fn test_higher_degree_product_constraint() {
        // n^2 * m with n, m <= 1e6, n * m <= 1e6 => worst case (n*m) * n = 1e6 * 1e6 = 1e12
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let n2 = power(n, 2.0).unwrap();
        let expr = multiply(vec![n2, m]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 1e6));
        cs.product_constraints.push(ProductConstraint::new(
            vec!["n".to_string(), "m".to_string()],
            1e6,
        ));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(1e12));
        assert_eq!(res.method, Some("product-constraint-bound".to_string()));
        assert!(res
            .assumptions
            .iter()
            .any(|a| a.contains("remaining factors evaluated at individual maxima")));
    }

    #[test]
    fn test_higher_degree_product_both_variables() {
        // n^2 * m^2 with n, m <= 1e6, n * m <= 1e6 => (n * m)^2 <= (1e6)^2 = 1e12
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let n2 = power(n, 2.0).unwrap();
        let m2 = power(m, 2.0).unwrap();
        let expr = multiply(vec![n2, m2]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 1e6));
        cs.product_constraints.push(ProductConstraint::new(
            vec!["n".to_string(), "m".to_string()],
            1e6,
        ));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(1e12));
        assert_eq!(res.method, Some("product-constraint-bound".to_string()));
    }

    #[test]
    fn test_linear_constraint_optimizes_product() {
        // x * y subject to x + y <= 200000, x, y <= 1e6 => max x * y = 10^5 * 10^5 = 10^10
        let x = variable("x").unwrap();
        let y = variable("y").unwrap();
        let expr = multiply(vec![x, y]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("x".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("y".to_string(), VariableBound::range(1.0, 1e6));

        let mut coeffs = HashMap::new();
        coeffs.insert("x".to_string(), 1.0);
        coeffs.insert("y".to_string(), 1.0);
        cs.linear_constraints
            .push(LinearConstraint::new(coeffs, 200000.0));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(1e10));
        assert_eq!(res.method, Some("linear-constraint-bound".to_string()));
    }

    #[test]
    fn test_linear_constraint_mismatched_powers() {
        // n^2 * m subject to n + m <= 300000 => n* = 2/3 * 3e5 = 2e5, m* = 1e5 => (2e5)^2 * 1e5 = 4e15
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let n2 = power(n, 2.0).unwrap();
        let expr = multiply(vec![n2, m]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 1e6));

        let mut coeffs = HashMap::new();
        coeffs.insert("n".to_string(), 1.0);
        coeffs.insert("m".to_string(), 1.0);
        cs.linear_constraints
            .push(LinearConstraint::new(coeffs, 300000.0));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(4e15));
        assert_eq!(res.method, Some("linear-constraint-bound".to_string()));
    }

    #[test]
    fn test_linear_sum_tightening() {
        // n + m subject to n + m <= 200000, n, m <= 1e6 => 200000
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let expr = add(vec![n, m]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1e6));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 1e6));

        let mut coeffs = HashMap::new();
        coeffs.insert("n".to_string(), 1.0);
        coeffs.insert("m".to_string(), 1.0);
        cs.linear_constraints
            .push(LinearConstraint::new(coeffs, 200000.0));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(200000.0));
        assert_eq!(res.method, Some("linear-constraint-bound".to_string()));
    }

    #[test]
    fn test_separable_sum_constraints() {
        // t * (n + m) subject to t <= 10000, n, m <= 200000, sum n <= 200000, sum m <= 300000
        // Result = 200000 + 300000 = 500000
        let t = variable("t").unwrap();
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let expr = multiply(vec![t, add(vec![n, m])]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("t".to_string(), VariableBound::range(1.0, 10000.0));
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 200000.0));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 200000.0));
        cs.sum_constraints
            .push(SumConstraint::new("n", "t", 200000.0));
        cs.sum_constraints
            .push(SumConstraint::new("m", "t", 300000.0));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(500000.0));
        assert_eq!(res.method, Some("sum-constraint-packing".to_string()));
    }

    #[test]
    fn test_separable_sum_with_higher_powers() {
        // t * (n^2 + m) with sum n <= 5000 (n <= 1000), sum m <= 4000 (m <= 2000), t <= 100
        // n^2 packs: 5 full cases of 1000 -> 5 * 1000^2 = 5e6. m is linear: 4000. Total = 5004000.
        let t = variable("t").unwrap();
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let n2 = power(n, 2.0).unwrap();
        let expr = multiply(vec![t, add(vec![n2, m])]);

        let mut cs = ConstraintSet::default();
        cs.variables
            .insert("t".to_string(), VariableBound::range(1.0, 100.0));
        cs.variables
            .insert("n".to_string(), VariableBound::range(1.0, 1000.0));
        cs.variables
            .insert("m".to_string(), VariableBound::range(1.0, 2000.0));
        cs.sum_constraints
            .push(SumConstraint::new("n", "t", 5000.0));
        cs.sum_constraints
            .push(SumConstraint::new("m", "t", 4000.0));

        let res = evaluate_workload_tightened(&expr, &cs);
        assert_eq!(res.status, BoundStatus::Proven);
        assert_eq!(res.upper_bound, Some(5004000.0));
        assert_eq!(res.method, Some("sum-constraint-packing".to_string()));
    }

    #[test]
    fn test_superadditivity_helper() {
        let n = variable("n").unwrap();
        let n2 = power(n.clone(), 2.0).unwrap();
        assert!(is_superadditive_in(&n2, "n"));
        assert!(is_superadditive_in(&n, "n"));

        let n_log = multiply(vec![n.clone(), logarithm(n.clone(), 2.0).unwrap()]);
        assert!(is_superadditive_in(&n_log, "n"));

        let log_n = logarithm(n.clone(), 2.0).unwrap();
        assert!(!is_superadditive_in(&log_n, "n"));

        let n_plus_one = add(vec![n, constant(1.0).unwrap()]);
        assert!(!is_superadditive_in(&n_plus_one, "n"));
    }
}
