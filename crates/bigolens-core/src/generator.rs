//! Constraint-driven deterministic input generator for dynamic benchmarking.
//!
//! Generates input streams parameterised by constraint variables (e.g. `n`, `t`),
//! ensuring all values and test case sizes strictly respect configured bounds,
//! with deterministic output from a fixed PRNG seed.
//! Conforms to BL-403 and BENCHMARK_SPECIFICATION.md.

use crate::constraints::ConstraintSet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Minimal self-contained 64-bit SplitMix64 pseudo-random number generator.
///
/// Guaranteed to be 100% deterministic and byte-identical across platforms.
#[derive(Debug, Clone)]
pub struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0x9e3779b97f4a7c15 } else { seed },
        }
    }

    /// Generate the next pseudo-random 64-bit integer.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Generate a pseudo-random integer in the closed range [min, max].
    pub fn gen_range(&mut self, min: i64, max: i64) -> i64 {
        if min >= max {
            return min;
        }
        let range = (max - min) as u64 + 1;
        let rand_val = self.next_u64() % range;
        min + rand_val as i64
    }
}

/// Declarative schema of benchmark input format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputFormat {
    /// Single integer value `v` in range [min, max]
    SingleInteger {
        variable: String,
        min: i64,
        max: i64,
    },
    /// An array of `size_variable` integers with elements in [min_val, max_val]
    Array {
        size_variable: String,
        min_val: i64,
        max_val: i64,
    },
    /// Multiple test cases: count `t_variable`, each with an inner format
    TestCases {
        count_variable: String,
        case_format: Box<InputFormat>,
    },
    /// Sum-constrained test cases: t test cases where sum of sizes <= max_total
    SumConstrainedCases {
        count_variable: String,
        size_variable: String,
        max_total: u64,
        min_val: i64,
        max_val: i64,
    },
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GeneratorError {
    #[error("variable '{0}' size {1} exceeds configured maximum bound {2}")]
    SizeExceedsMaximum(String, u64, u64),
    #[error("variable '{0}' not assigned a size")]
    MissingVariable(String),
    #[error("sum constraint exceeded: total sizes {0} > max_total {1}")]
    SumConstraintExceeded(u64, u64),
    #[error("input byte limit exceeded (generated {0} bytes > limit {1})")]
    ByteLimitExceeded(usize, usize),
}

/// Configuration for input generation.
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    pub seed: u64,
    pub byte_limit: usize,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            byte_limit: 16 * 1024 * 1024, // 16 MiB max input size
        }
    }
}

/// Generate deterministic benchmark input based on declarative format and variable assignments.
pub fn generate_input(
    format: &InputFormat,
    sizes: &HashMap<String, u64>,
    constraints: &ConstraintSet,
    config: &GeneratorConfig,
) -> Result<String, GeneratorError> {
    // 1. Verify bounds: sizes must not exceed configured maxima
    for (var, &val) in sizes {
        if let Some(bound) = constraints.variables.get(var) {
            if let Some(max_val) = bound.max {
                if (val as f64) > max_val {
                    return Err(GeneratorError::SizeExceedsMaximum(
                        var.clone(),
                        val,
                        max_val as u64,
                    ));
                }
            }
        }
    }

    let mut rng = DeterministicRng::new(config.seed);
    let mut output = String::new();

    generate_format(format, sizes, constraints, &mut rng, &mut output, config)?;

    if output.len() > config.byte_limit {
        return Err(GeneratorError::ByteLimitExceeded(
            output.len(),
            config.byte_limit,
        ));
    }

    Ok(output)
}

fn generate_format(
    format: &InputFormat,
    sizes: &HashMap<String, u64>,
    _constraints: &ConstraintSet,
    rng: &mut DeterministicRng,
    out: &mut String,
    config: &GeneratorConfig,
) -> Result<(), GeneratorError> {
    match format {
        InputFormat::SingleInteger { min, max, .. } => {
            let val = rng.gen_range(*min, *max);
            out.push_str(&val.to_string());
            out.push('\n');
        }
        InputFormat::Array {
            size_variable,
            min_val,
            max_val,
        } => {
            let count = *sizes
                .get(size_variable)
                .ok_or_else(|| GeneratorError::MissingVariable(size_variable.clone()))?;

            out.push_str(&count.to_string());
            out.push('\n');

            for i in 0..count {
                if out.len() > config.byte_limit {
                    return Err(GeneratorError::ByteLimitExceeded(
                        out.len(),
                        config.byte_limit,
                    ));
                }
                let elem = rng.gen_range(*min_val, *max_val);
                out.push_str(&elem.to_string());
                if i + 1 < count {
                    out.push(' ');
                } else {
                    out.push('\n');
                }
            }
        }
        InputFormat::TestCases {
            count_variable,
            case_format,
        } => {
            let t = *sizes
                .get(count_variable)
                .ok_or_else(|| GeneratorError::MissingVariable(count_variable.clone()))?;

            out.push_str(&t.to_string());
            out.push('\n');

            for _ in 0..t {
                generate_format(case_format, sizes, _constraints, rng, out, config)?;
            }
        }
        InputFormat::SumConstrainedCases {
            count_variable,
            size_variable,
            max_total,
            min_val,
            max_val,
        } => {
            let t = *sizes
                .get(count_variable)
                .ok_or_else(|| GeneratorError::MissingVariable(count_variable.clone()))?;
            let n_target = sizes
                .get(size_variable)
                .copied()
                .unwrap_or(max_total / t.max(1));

            // Distribute sizes across t cases such that each <= n_target and sum <= max_total
            let mut allocated = 0u64;
            let mut case_sizes = Vec::with_capacity(t as usize);

            for i in 0..t {
                let remaining_cases = t - i;
                let max_possible = (*max_total).saturating_sub(allocated);
                let this_size = (max_possible / remaining_cases).min(n_target).max(1);
                case_sizes.push(this_size);
                allocated += this_size;
            }

            if allocated > *max_total {
                return Err(GeneratorError::SumConstraintExceeded(allocated, *max_total));
            }

            out.push_str(&t.to_string());
            out.push('\n');

            for &n_i in &case_sizes {
                out.push_str(&n_i.to_string());
                out.push('\n');
                for j in 0..n_i {
                    if out.len() > config.byte_limit {
                        return Err(GeneratorError::ByteLimitExceeded(
                            out.len(),
                            config.byte_limit,
                        ));
                    }
                    let val = rng.gen_range(*min_val, *max_val);
                    out.push_str(&val.to_string());
                    if j + 1 < n_i {
                        out.push(' ');
                    } else {
                        out.push('\n');
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::VariableBound;

    #[test]
    fn test_determinism_same_seed_produces_byte_identical_output() {
        let format = InputFormat::Array {
            size_variable: "n".to_string(),
            min_val: 1,
            max_val: 100,
        };

        let mut sizes = HashMap::new();
        sizes.insert("n".to_string(), 50);

        let constraints = ConstraintSet::default();
        let config1 = GeneratorConfig {
            seed: 12345,
            byte_limit: 1024 * 1024,
        };
        let config2 = GeneratorConfig {
            seed: 12345,
            byte_limit: 1024 * 1024,
        };

        let out1 = generate_input(&format, &sizes, &constraints, &config1).unwrap();
        let out2 = generate_input(&format, &sizes, &constraints, &config2).unwrap();

        assert_eq!(out1, out2);
        assert!(out1.starts_with("50\n"));
    }

    #[test]
    fn test_generated_values_respect_configured_bounds() {
        let min_val = 10;
        let max_val = 20;
        let format = InputFormat::Array {
            size_variable: "n".to_string(),
            min_val,
            max_val,
        };

        let mut sizes = HashMap::new();
        sizes.insert("n".to_string(), 100);

        let constraints = ConstraintSet::default();
        let config = GeneratorConfig {
            seed: 999,
            byte_limit: 1024 * 1024,
        };

        let out = generate_input(&format, &sizes, &constraints, &config).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "100");

        let values: Vec<i64> = lines[1]
            .split_whitespace()
            .map(|s| s.parse().unwrap())
            .collect();
        assert_eq!(values.len(), 100);
        for &v in &values {
            assert!(
                v >= min_val && v <= max_val,
                "value {} out of bounds [{}, {}]",
                v,
                min_val,
                max_val
            );
        }
    }

    #[test]
    fn test_sizes_above_constraint_maxima_are_rejected() {
        let format = InputFormat::Array {
            size_variable: "n".to_string(),
            min_val: 0,
            max_val: 10,
        };

        let mut constraints = ConstraintSet::default();
        constraints.variables.insert(
            "n".to_string(),
            VariableBound {
                min: Some(1.0),
                max: Some(1000.0), // maximum allowed is 1000
            },
        );

        let mut sizes = HashMap::new();
        sizes.insert("n".to_string(), 1500); // 1500 > 1000

        let config = GeneratorConfig::default();
        let res = generate_input(&format, &sizes, &constraints, &config);

        assert_eq!(
            res.unwrap_err(),
            GeneratorError::SizeExceedsMaximum("n".to_string(), 1500, 1000)
        );
    }

    #[test]
    fn test_sum_constraint_distribution_across_test_cases() {
        let format = InputFormat::SumConstrainedCases {
            count_variable: "t".to_string(),
            size_variable: "n".to_string(),
            max_total: 200_000,
            min_val: 1,
            max_val: 1_000_000,
        };

        let mut sizes = HashMap::new();
        sizes.insert("t".to_string(), 10);
        sizes.insert("n".to_string(), 50_000);

        let constraints = ConstraintSet::default();
        let config = GeneratorConfig {
            seed: 777,
            byte_limit: 10 * 1024 * 1024,
        };

        let out = generate_input(&format, &sizes, &constraints, &config).unwrap();
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "10"); // t = 10

        let mut total_n = 0u64;
        let mut idx = 1;
        for _ in 0..10 {
            let n_i: u64 = lines[idx].parse().unwrap();
            total_n += n_i;
            idx += 2;
        }

        assert!(
            total_n <= 200_000,
            "total n {} exceeded sum constraint 200000",
            total_n
        );
    }
}
