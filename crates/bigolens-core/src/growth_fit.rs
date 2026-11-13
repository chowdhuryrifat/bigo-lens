//! Empirical growth model fitting for benchmark runtime measurements.
//!
//! Fits candidate growth models ($O(n)$, $O(n \log n)$, $O(n^2)$) to measured
//! execution times using ordinary least squares with feature scaling.
//!
//! Conforms to `BENCHMARK_SPECIFICATION.md` and matches the C++ reference in
//! `benchmark/include/bigolens/benchmark/growth_fit.hpp`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Candidate growth model for empirical scaling analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrowthModel {
    /// Linear scaling: $O(n)$
    Linear,
    /// Linearithmic scaling: $O(n \log n)$
    Linearithmic,
    /// Quadratic scaling: $O(n^2)$
    Quadratic,
}

impl GrowthModel {
    /// Returns the canonical string representation of the model.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Linearithmic => "linearithmic",
            Self::Quadratic => "quadratic",
        }
    }
}

impl fmt::Display for GrowthModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// One measured point: input size `n` and median runtime in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Measurement {
    /// Input size ($n > 0$).
    pub n: f64,
    /// Measured execution time in milliseconds ($\ge 0$).
    pub milliseconds: f64,
}

impl Measurement {
    /// Creates a new measurement.
    pub fn new(n: f64, milliseconds: f64) -> Self {
        Self { n, milliseconds }
    }
}

/// Least-squares fit of $t(n) = a + b \cdot f(n)$ for one candidate model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelFit {
    /// Candidate growth model.
    pub model: GrowthModel,
    /// Intercept parameter ($a$).
    pub intercept: f64,
    /// Slope parameter ($b$).
    pub slope: f64,
    /// Residual sum of squares.
    pub rss: f64,
    /// Coefficient of determination ($R^2$).
    pub r2: f64,
}

/// Result of growth model fitting across all candidate models.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrowthFitResult {
    /// All candidate fits that survived slope filtering, ordered ascending by RSS (best first).
    pub fits: Vec<ModelFit>,
    /// Selected model, or `None` if data cannot discriminate cleanly.
    pub selected: Option<GrowthModel>,
    /// `true` if runner-up fits nearly as well or if data does not discriminate.
    pub ambiguous: bool,
}

/// Options controlling empirical growth fitting and discrimination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrowthFitOptions {
    /// Minimum number of distinct input sizes required to fit (clamped to at least 3).
    pub minimum_points: usize,
    /// The best model must have an RSS at most this fraction of the runner-up's.
    pub discrimination_ratio: f64,
}

impl Default for GrowthFitOptions {
    fn default() -> Self {
        Self {
            minimum_points: 4,
            discrimination_ratio: 0.5,
        }
    }
}

/// Evaluates the feature function $f(n)$ for a given growth model.
pub fn feature(model: GrowthModel, n: f64) -> f64 {
    match model {
        GrowthModel::Linear => n,
        GrowthModel::Linearithmic => n * f64::log2(n.max(2.0)),
        GrowthModel::Quadratic => n * n,
    }
}

/// Predicts runtime $t(n) = a + b \cdot f(n)$ from a model fit.
pub fn predict(fit: &ModelFit, n: f64) -> f64 {
    fit.intercept + fit.slope * feature(fit.model, n)
}

/// Fits candidate models to measurements using ordinary least squares.
///
/// Features are scaled by $\max(x)$ to keep the normal equations well-conditioned.
///
/// Returns `None` if:
/// - any measurement has non-finite values,
/// - any $n \le 0.0$,
/// - any milliseconds $< 0.0$,
/// - there are fewer distinct sizes than `options.minimum_points.max(3)`.
pub fn fit_growth(points: &[Measurement], options: &GrowthFitOptions) -> Option<GrowthFitResult> {
    for p in points {
        if !p.n.is_finite() || !p.milliseconds.is_finite() || p.n <= 0.0 || p.milliseconds < 0.0 {
            return None;
        }
    }

    let mut distinct_sizes: Vec<f64> = points.iter().map(|p| p.n).collect();
    distinct_sizes.sort_by(|a, b| a.total_cmp(b));
    distinct_sizes.dedup();
    if distinct_sizes.len() < options.minimum_points.max(3) {
        return None;
    }

    let mut fits = Vec::new();
    for model in [
        GrowthModel::Linear,
        GrowthModel::Linearithmic,
        GrowthModel::Quadratic,
    ] {
        let fit = fit_one(model, points);
        // A negative slope means runtime decreases with n: physically implausible for these models.
        if fit.slope >= 0.0 {
            fits.push(fit);
        }
    }

    fits.sort_by(|a, b| a.rss.total_cmp(&b.rss));
    if fits.is_empty() {
        return Some(GrowthFitResult {
            fits,
            selected: None,
            ambiguous: true,
        });
    }

    // Flat data (no measurable growth) or tied residuals cannot discriminate between models.
    let grows = fits[0].slope > 0.0;
    let discriminates =
        grows && (fits.len() == 1 || fits[0].rss < options.discrimination_ratio * fits[1].rss);
    let ambiguous = !discriminates;
    let selected = if discriminates {
        Some(fits[0].model)
    } else {
        None
    };

    Some(GrowthFitResult {
        fits,
        selected,
        ambiguous,
    })
}

fn fit_one(model: GrowthModel, points: &[Measurement]) -> ModelFit {
    // Ordinary least squares for t = a + b * x, x = f(n).
    // Features are scaled to keep normal equations well conditioned.
    let mut max_x = 0.0f64;
    for p in points {
        max_x = max_x.max(feature(model, p.n));
    }
    let scale = if max_x > 0.0 { max_x } else { 1.0 };
    let count = points.len() as f64;

    let mut sum_x = 0.0f64;
    let mut sum_y = 0.0f64;
    for p in points {
        sum_x += feature(model, p.n) / scale;
        sum_y += p.milliseconds;
    }
    let mean_x = sum_x / count;
    let mean_y = sum_y / count;

    let mut sxx = 0.0f64;
    let mut sxy = 0.0f64;
    let mut syy = 0.0f64;
    for p in points {
        let dx = feature(model, p.n) / scale - mean_x;
        let dy = p.milliseconds - mean_y;
        sxx += dx * dx;
        sxy += dx * dy;
        syy += dy * dy;
    }

    let scaled_slope = if sxx > 0.0 { sxy / sxx } else { 0.0 };
    let slope = scaled_slope / scale;
    let intercept = mean_y - scaled_slope * mean_x;

    let mut fit = ModelFit {
        model,
        intercept,
        slope,
        rss: 0.0,
        r2: 0.0,
    };

    let mut rss = 0.0f64;
    for p in points {
        let residual = p.milliseconds - predict(&fit, p.n);
        rss += residual * residual;
    }
    fit.rss = rss;
    fit.r2 = if syy > 0.0 { 1.0 - rss / syy } else { 0.0 };

    fit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic<F: Fn(f64) -> f64>(f: F) -> Vec<Measurement> {
        let mut points = Vec::new();
        let mut n = 1000.0;
        while n <= 64000.0 {
            points.push(Measurement {
                n,
                milliseconds: 0.5 + f(n),
            });
            n *= 2.0;
        }
        points
    }

    #[test]
    fn test_selects_quadratic_for_quadratic_data() {
        let options = GrowthFitOptions::default();
        let result = fit_growth(&synthetic(|n| 1e-6 * n * n), &options);
        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.selected, Some(GrowthModel::Quadratic));
        assert!(!res.ambiguous);
        assert!((res.fits[0].slope - 1e-6).abs() < 1e-9);
    }

    #[test]
    fn test_selects_linear_for_linear_data() {
        let options = GrowthFitOptions::default();
        let result = fit_growth(&synthetic(|n| 2e-3 * n), &options);
        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.selected, Some(GrowthModel::Linear));
        assert!(!res.ambiguous);
    }

    #[test]
    fn test_selects_linearithmic_for_n_log_n_data() {
        let options = GrowthFitOptions::default();
        let result = fit_growth(&synthetic(|n| 1e-4 * n * f64::log2(n)), &options);
        assert!(result.is_some());
        let res = result.unwrap();
        assert_eq!(res.selected, Some(GrowthModel::Linearithmic));
        assert!(!res.ambiguous);
    }

    #[test]
    fn test_constant_data_is_ambiguous() {
        let options = GrowthFitOptions::default();
        let mut points = Vec::new();
        let mut n = 1000.0;
        while n <= 64000.0 {
            points.push(Measurement {
                n,
                milliseconds: 3.0,
            });
            n *= 2.0;
        }
        let result = fit_growth(&points, &options);
        assert!(result.is_some());
        let res = result.unwrap();
        assert!(res.ambiguous);
        assert_eq!(res.selected, None);
    }

    #[test]
    fn test_rejects_too_few_or_invalid_points() {
        let options = GrowthFitOptions::default();
        assert!(fit_growth(
            &[
                Measurement {
                    n: 1.0,
                    milliseconds: 1.0
                },
                Measurement {
                    n: 2.0,
                    milliseconds: 2.0
                },
                Measurement {
                    n: 3.0,
                    milliseconds: 3.0
                },
            ],
            &options
        )
        .is_none());

        assert!(fit_growth(
            &[
                Measurement {
                    n: 1.0,
                    milliseconds: 1.0
                },
                Measurement {
                    n: 2.0,
                    milliseconds: 2.0
                },
                Measurement {
                    n: 3.0,
                    milliseconds: 3.0
                },
                Measurement {
                    n: -4.0,
                    milliseconds: 4.0
                },
            ],
            &options
        )
        .is_none());

        assert!(fit_growth(
            &[
                Measurement {
                    n: 1.0,
                    milliseconds: 1.0
                },
                Measurement {
                    n: 1.0,
                    milliseconds: 2.0
                },
                Measurement {
                    n: 1.0,
                    milliseconds: 3.0
                },
                Measurement {
                    n: 1.0,
                    milliseconds: 4.0
                },
            ],
            &options
        )
        .is_none());
    }

    #[test]
    fn test_growth_model_as_str_and_display() {
        assert_eq!(GrowthModel::Linear.as_str(), "linear");
        assert_eq!(GrowthModel::Linearithmic.as_str(), "linearithmic");
        assert_eq!(GrowthModel::Quadratic.as_str(), "quadratic");

        assert_eq!(GrowthModel::Linear.to_string(), "linear");
        assert_eq!(GrowthModel::Linearithmic.to_string(), "linearithmic");
        assert_eq!(GrowthModel::Quadratic.to_string(), "quadratic");
    }

    #[test]
    fn test_serde_serialization() {
        let model = GrowthModel::Linearithmic;
        let json = serde_json::to_string(&model).expect("serialize GrowthModel");
        assert_eq!(json, "\"linearithmic\"");
        let deserialized: GrowthModel =
            serde_json::from_str(&json).expect("deserialize GrowthModel");
        assert_eq!(deserialized, model);

        let measurement = Measurement::new(100.0, 12.5);
        let m_json = serde_json::to_string(&measurement).expect("serialize Measurement");
        let m_de: Measurement = serde_json::from_str(&m_json).expect("deserialize Measurement");
        assert_eq!(m_de, measurement);

        let options = GrowthFitOptions::default();
        let opt_json = serde_json::to_string(&options).expect("serialize GrowthFitOptions");
        let opt_de: GrowthFitOptions =
            serde_json::from_str(&opt_json).expect("deserialize GrowthFitOptions");
        assert_eq!(opt_de, options);
    }

    #[test]
    fn test_predict_and_feature() {
        let fit = ModelFit {
            model: GrowthModel::Linear,
            intercept: 5.0,
            slope: 2.0,
            rss: 0.1,
            r2: 0.99,
        };
        assert_eq!(predict(&fit, 10.0), 25.0);

        let quad_fit = ModelFit {
            model: GrowthModel::Quadratic,
            intercept: 1.0,
            slope: 3.0,
            rss: 0.05,
            r2: 0.995,
        };
        assert_eq!(predict(&quad_fit, 4.0), 1.0 + 3.0 * 16.0);
    }

    #[test]
    fn test_rejects_nan_and_infinite_values() {
        let options = GrowthFitOptions::default();
        let valid = [
            Measurement::new(1.0, 1.0),
            Measurement::new(2.0, 2.0),
            Measurement::new(3.0, 3.0),
            Measurement::new(4.0, 4.0),
        ];

        let mut nan_n = valid.to_vec();
        nan_n[0].n = f64::NAN;
        assert!(fit_growth(&nan_n, &options).is_none());

        let mut inf_n = valid.to_vec();
        inf_n[0].n = f64::INFINITY;
        assert!(fit_growth(&inf_n, &options).is_none());

        let mut zero_n = valid.to_vec();
        zero_n[0].n = 0.0;
        assert!(fit_growth(&zero_n, &options).is_none());

        let mut neg_ms = valid.to_vec();
        neg_ms[0].milliseconds = -0.5;
        assert!(fit_growth(&neg_ms, &options).is_none());

        let mut nan_ms = valid.to_vec();
        nan_ms[0].milliseconds = f64::NAN;
        assert!(fit_growth(&nan_ms, &options).is_none());
    }
}
