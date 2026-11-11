//! Sample statistics for benchmark measurements.
//!
//! Provides robust estimation of central tendency and dispersion for repeated
//! execution times (minimum, median, maximum, median absolute deviation (MAD),
//! and relative spread).
//!
//! Follows BENCHMARK_SPECIFICATION.md §5 and ADR 0014.

use serde::{Deserialize, Serialize};

/// Robust summary of repeated runtime measurements (typically milliseconds).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampleSummary {
    /// Number of valid samples in the dataset.
    pub count: usize,
    /// Smallest observed sample value.
    pub minimum: f64,
    /// Median value (middle element for odd counts, mean of middle two for even counts).
    pub median: f64,
    /// Largest observed sample value.
    pub maximum: f64,
    /// Median absolute deviation from the median (unscaled).
    pub mad: f64,
    /// Relative spread: `mad / median` (or `0.0` when median is `0.0`).
    pub relative_spread: f64,
}

/// Compute the median of a non-empty slice of numbers.
///
/// Modifies the provided vector in-place.
pub fn median(values: &mut [f64]) -> f64 {
    assert!(!values.is_empty(), "median requires at least one value");
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        values[mid - 1] + (values[mid] - values[mid - 1]) / 2.0
    }
}

/// Robust summary of a sample vector.
///
/// Returns `None` if `samples` is empty, or if any sample is negative or not finite
/// (measurements must never be fabricated or silently repaired).
pub fn summarize(samples: &[f64]) -> Option<SampleSummary> {
    if samples.is_empty() {
        return None;
    }
    for &s in samples {
        if !s.is_finite() || s < 0.0 {
            return None;
        }
    }

    let mut sorted = samples.to_vec();
    let med = median(&mut sorted);

    let min_val = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max_val = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let mut deviations: Vec<f64> = samples.iter().map(|s| (s - med).abs()).collect();
    let mad_val = median(&mut deviations);

    let relative_spread = if med > 0.0 { mad_val / med } else { 0.0 };

    Some(SampleSummary {
        count: samples.len(),
        minimum: min_val,
        median: med,
        maximum: max_val,
        mad: mad_val,
        relative_spread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_odd_count_median_and_mad() {
        let s = summarize(&[5.0, 1.0, 3.0, 2.0, 4.0]).expect("expected summary");
        assert_eq!(s.count, 5);
        assert!((s.median - 3.0).abs() < 1e-12);
        assert!((s.minimum - 1.0).abs() < 1e-12);
        assert!((s.maximum - 5.0).abs() < 1e-12);
        assert!((s.mad - 1.0).abs() < 1e-12);
        assert!((s.relative_spread - (1.0 / 3.0)).abs() < 1e-12);
    }

    #[test]
    fn test_even_count_median_is_midpoint() {
        let s = summarize(&[4.0, 1.0, 3.0, 2.0]).expect("expected summary");
        assert_eq!(s.count, 4);
        assert!((s.median - 2.5).abs() < 1e-12);
        assert!((s.minimum - 1.0).abs() < 1e-12);
        assert!((s.maximum - 4.0).abs() < 1e-12);
    }

    #[test]
    fn test_outlier_does_not_move_median() {
        let s = summarize(&[10.0, 10.0, 10.0, 10.0, 1000.0]).expect("expected summary");
        assert_eq!(s.count, 5);
        assert!((s.median - 10.0).abs() < 1e-12);
        assert!((s.mad - 0.0).abs() < 1e-12);
        assert!((s.relative_spread - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_rejects_invalid_samples() {
        assert!(summarize(&[]).is_none());
        assert!(summarize(&[1.0, -1.0]).is_none());
        assert!(summarize(&[1.0, f64::NAN]).is_none());
        assert!(summarize(&[f64::INFINITY]).is_none());
        assert!(summarize(&[f64::NEG_INFINITY]).is_none());
    }

    #[test]
    fn test_single_element() {
        let s = summarize(&[42.0]).expect("expected summary");
        assert_eq!(s.count, 1);
        assert!((s.median - 42.0).abs() < 1e-12);
        assert!((s.minimum - 42.0).abs() < 1e-12);
        assert!((s.maximum - 42.0).abs() < 1e-12);
        assert!((s.mad - 0.0).abs() < 1e-12);
        assert!((s.relative_spread - 0.0).abs() < 1e-12);
    }

    #[test]
    fn test_all_zeros() {
        let s = summarize(&[0.0, 0.0, 0.0]).expect("expected summary");
        assert_eq!(s.count, 3);
        assert_eq!(s.median, 0.0);
        assert_eq!(s.mad, 0.0);
        assert_eq!(s.relative_spread, 0.0);
    }
}
