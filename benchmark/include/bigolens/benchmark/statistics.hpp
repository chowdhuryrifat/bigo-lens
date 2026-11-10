#pragma once

#include <optional>
#include <vector>

namespace bigolens::benchmark {

/// Robust summary of repeated runtime measurements (milliseconds).
struct SampleSummary {
  std::size_t count = 0;
  double minimum = 0.0;
  double median = 0.0;
  double maximum = 0.0;
  /// Median absolute deviation from the median.
  double mad = 0.0;
  /// mad / median; 0 when the median is 0.
  double relativeSpread = 0.0;
};

/// Summarises samples. Returns std::nullopt for an empty input or if any
/// sample is negative or not finite (measurements must never be fabricated or
/// silently repaired).
[[nodiscard]] std::optional<SampleSummary> summarize(std::vector<double> samples);

/// Median of a non-empty vector (the vector is reordered).
[[nodiscard]] double median(std::vector<double>& values);

}  // namespace bigolens::benchmark
