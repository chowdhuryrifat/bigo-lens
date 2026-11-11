//! Symbolic expression trees and algebraic evaluation.

use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ComplexityError {
    #[error("non-finite constant or calculation")]
    NonFinite,
    #[error("variable name cannot be empty")]
    EmptyVariableName,
    #[error("power exponent must be non-negative")]
    InvalidExponent,
    #[error("logarithm base must be > 1.0")]
    InvalidLogBase,
    #[error("evaluation budget exceeded")]
    BudgetExceeded,
    #[error("unbound variable: {0}")]
    UnboundVariable(String),
}

/// Symbolic expression tree node.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Constant(f64),
    Variable(String),
    Add(Vec<Arc<Expr>>),
    Multiply(Vec<Arc<Expr>>),
    Power {
        base: Arc<Expr>,
        exponent: f64,
    },
    Logarithm {
        arg: Arc<Expr>,
        base: f64,
    },
    Summation {
        index: String,
        lower: Arc<Expr>,
        upper: Arc<Expr>,
        body: Arc<Expr>,
    },
    Maximum(Vec<Arc<Expr>>),
}

pub type ExprPtr = Arc<Expr>;

pub fn constant(val: f64) -> Result<ExprPtr, ComplexityError> {
    if !val.is_finite() {
        return Err(ComplexityError::NonFinite);
    }
    Ok(Arc::new(Expr::Constant(val)))
}

pub fn variable(name: impl Into<String>) -> Result<ExprPtr, ComplexityError> {
    let s = name.into();
    if s.is_empty() {
        return Err(ComplexityError::EmptyVariableName);
    }
    Ok(Arc::new(Expr::Variable(s)))
}

pub fn add(terms: Vec<ExprPtr>) -> ExprPtr {
    let mut flat = Vec::new();
    let mut const_sum = 0.0;

    for term in terms {
        match term.as_ref() {
            Expr::Add(inner) => {
                for t in inner {
                    if let Expr::Constant(c) = t.as_ref() {
                        const_sum += c;
                    } else {
                        flat.push(Arc::clone(t));
                    }
                }
            }
            Expr::Constant(c) => const_sum += c,
            _ => flat.push(term),
        }
    }

    if const_sum != 0.0 || flat.is_empty() {
        flat.push(Arc::new(Expr::Constant(const_sum)));
    }

    if flat.len() == 1 {
        flat.pop().unwrap()
    } else {
        Arc::new(Expr::Add(flat))
    }
}

pub fn multiply(factors: Vec<ExprPtr>) -> ExprPtr {
    let mut flat = Vec::new();
    let mut const_prod = 1.0;

    for factor in factors {
        match factor.as_ref() {
            Expr::Multiply(inner) => {
                for f in inner {
                    if let Expr::Constant(c) = f.as_ref() {
                        const_prod *= c;
                    } else {
                        flat.push(Arc::clone(f));
                    }
                }
            }
            Expr::Constant(c) => const_prod *= c,
            _ => flat.push(factor),
        }
    }

    if const_prod == 0.0 {
        return Arc::new(Expr::Constant(0.0));
    }

    if const_prod != 1.0 || flat.is_empty() {
        flat.insert(0, Arc::new(Expr::Constant(const_prod)));
    }

    if flat.len() == 1 {
        flat.pop().unwrap()
    } else {
        Arc::new(Expr::Multiply(flat))
    }
}

pub fn subtract(lhs: ExprPtr, rhs: ExprPtr) -> ExprPtr {
    add(vec![
        lhs,
        multiply(vec![Arc::new(Expr::Constant(-1.0)), rhs]),
    ])
}

pub fn power(base: ExprPtr, exponent: f64) -> Result<ExprPtr, ComplexityError> {
    if !exponent.is_finite() || exponent < 0.0 {
        return Err(ComplexityError::InvalidExponent);
    }
    if exponent == 0.0 {
        return Ok(Arc::new(Expr::Constant(1.0)));
    }
    if exponent == 1.0 {
        return Ok(base);
    }
    if let Expr::Constant(c) = base.as_ref() {
        return Ok(Arc::new(Expr::Constant(c.powf(exponent))));
    }
    Ok(Arc::new(Expr::Power { base, exponent }))
}

pub fn logarithm(arg: ExprPtr, base: f64) -> Result<ExprPtr, ComplexityError> {
    if !base.is_finite() || base <= 1.0 {
        return Err(ComplexityError::InvalidLogBase);
    }
    Ok(Arc::new(Expr::Logarithm { arg, base }))
}

impl Expr {
    /// Collect all free (unbound) variable names in this expression.
    pub fn free_variables(&self) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        self.collect_free(&mut set, &mut BTreeSet::new());
        set
    }

    fn collect_free(&self, out: &mut BTreeSet<String>, bound: &mut BTreeSet<String>) {
        match self {
            Expr::Constant(_) => {}
            Expr::Variable(name) => {
                if !bound.contains(name) {
                    out.insert(name.clone());
                }
            }
            Expr::Add(ops) | Expr::Multiply(ops) | Expr::Maximum(ops) => {
                for op in ops {
                    op.collect_free(out, bound);
                }
            }
            Expr::Power { base, .. } | Expr::Logarithm { arg: base, .. } => {
                base.collect_free(out, bound);
            }
            Expr::Summation {
                index,
                lower,
                upper,
                body,
            } => {
                lower.collect_free(out, bound);
                upper.collect_free(out, bound);
                bound.insert(index.clone());
                body.collect_free(out, bound);
                bound.remove(index);
            }
        }
    }

    /// Substitute occurrences of `var` with `replacement`.
    pub fn substitute(&self, var: &str, replacement: &ExprPtr) -> ExprPtr {
        match self {
            Expr::Constant(_) => Arc::new(self.clone()),
            Expr::Variable(name) => {
                if name == var {
                    Arc::clone(replacement)
                } else {
                    Arc::new(self.clone())
                }
            }
            Expr::Add(ops) => add(ops
                .iter()
                .map(|op| op.substitute(var, replacement))
                .collect()),
            Expr::Multiply(ops) => multiply(
                ops.iter()
                    .map(|op| op.substitute(var, replacement))
                    .collect(),
            ),
            Expr::Power { base, exponent } => Arc::new(Expr::Power {
                base: base.substitute(var, replacement),
                exponent: *exponent,
            }),
            Expr::Logarithm { arg, base } => Arc::new(Expr::Logarithm {
                arg: arg.substitute(var, replacement),
                base: *base,
            }),
            Expr::Maximum(ops) => Arc::new(Expr::Maximum(
                ops.iter()
                    .map(|op| op.substitute(var, replacement))
                    .collect(),
            )),
            Expr::Summation {
                index,
                lower,
                upper,
                body,
            } => {
                let l = lower.substitute(var, replacement);
                let u = upper.substitute(var, replacement);
                let b = if index == var {
                    Arc::clone(body)
                } else {
                    body.substitute(var, replacement)
                };
                Arc::new(Expr::Summation {
                    index: index.clone(),
                    lower: l,
                    upper: u,
                    body: b,
                })
            }
        }
    }

    /// Numerically evaluate with given variable values.
    pub fn evaluate(&self, vars: &HashMap<String, f64>) -> Result<f64, ComplexityError> {
        match self {
            Expr::Constant(c) => Ok(*c),
            Expr::Variable(name) => vars
                .get(name)
                .copied()
                .ok_or_else(|| ComplexityError::UnboundVariable(name.clone())),
            Expr::Add(ops) => {
                let mut sum = 0.0;
                for op in ops {
                    sum += op.evaluate(vars)?;
                }
                if !sum.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(sum)
                }
            }
            Expr::Multiply(ops) => {
                let mut prod = 1.0;
                for op in ops {
                    prod *= op.evaluate(vars)?;
                }
                if !prod.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(prod)
                }
            }
            Expr::Power { base, exponent } => {
                let b = base.evaluate(vars)?;
                let res = b.powf(*exponent);
                if !res.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(res)
                }
            }
            Expr::Logarithm { arg, base } => {
                let a = arg.evaluate(vars)?.max(1.0);
                let res = a.log(*base);
                if !res.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(res)
                }
            }
            Expr::Maximum(ops) => {
                let mut max_val = f64::NEG_INFINITY;
                for op in ops {
                    let v = op.evaluate(vars)?;
                    if v > max_val {
                        max_val = v;
                    }
                }
                if !max_val.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(max_val)
                }
            }
            Expr::Summation {
                index,
                lower,
                upper,
                body,
            } => {
                let l = lower.evaluate(vars)?.round() as i64;
                let u = upper.evaluate(vars)?.round() as i64;
                let mut total = 0.0;
                let mut local_vars = vars.clone();
                for i in l..=u {
                    local_vars.insert(index.clone(), i as f64);
                    total += body.evaluate(&local_vars)?;
                }
                if !total.is_finite() {
                    Err(ComplexityError::NonFinite)
                } else {
                    Ok(total)
                }
            }
        }
    }

    /// Derive standard Big-O asymptotic notation string.
    pub fn asymptotic_bound(&self) -> String {
        let vars = self.free_variables();
        if vars.is_empty() {
            return "O(1)".to_string();
        }
        format!("O({})", self)
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Constant(c) => {
                if c.fract() == 0.0 && *c >= 0.0 && *c <= 1e15 {
                    write!(f, "{}", *c as u64)
                } else {
                    write!(f, "{}", c)
                }
            }
            Expr::Variable(name) => write!(f, "{}", name),
            Expr::Add(ops) => {
                for (i, op) in ops.iter().enumerate() {
                    if i > 0 {
                        write!(f, " + ")?;
                    }
                    write!(f, "{}", op)?;
                }
                Ok(())
            }
            Expr::Multiply(ops) => {
                for (i, op) in ops.iter().enumerate() {
                    if i > 0 {
                        write!(f, " * ")?;
                    }
                    let needs_paren = matches!(op.as_ref(), Expr::Add(_));
                    if needs_paren {
                        write!(f, "({})", op)?;
                    } else {
                        write!(f, "{}", op)?;
                    }
                }
                Ok(())
            }
            Expr::Power { base, exponent } => write!(f, "{}^{}", base, exponent),
            Expr::Logarithm { arg, base } => write!(f, "log_{}({})", base, arg),
            Expr::Summation {
                index,
                lower,
                upper,
                body,
            } => write!(f, "sum_{{{}={}}}^{{{}}} {}", index, lower, upper, body),
            Expr::Maximum(ops) => {
                write!(f, "max(")?;
                for (i, op) in ops.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", op)?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expression_algebra() {
        let n = variable("n").unwrap();
        let m = variable("m").unwrap();
        let c4 = constant(4.0).unwrap();
        let prod = multiply(vec![n, m, c4]);
        assert_eq!(prod.to_string(), "4 * n * m");
    }

    #[test]
    fn test_evaluation() {
        let n = variable("n").unwrap();
        let mut vars = HashMap::new();
        vars.insert("n".to_string(), 100.0);
        assert_eq!(n.evaluate(&vars).unwrap(), 100.0);
    }
}
