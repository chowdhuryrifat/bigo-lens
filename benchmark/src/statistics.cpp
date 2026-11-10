#include "bigolens/benchmark/statistics.hpp"

#include <algorithm>
#include <cmath>

namespace bigolens::benchmark {

double median(std::vector<double>& values) {
  const std::size_t mid = values.size() / 2;
  std::nth_element(values.begin(), values.begin() + static_cast<std::ptrdiff_t>(mid), values.end());
  const double upper = values[mid];
  if (values.size() % 2 == 1) {
    return upper;
  }
  const double lower = *std::max_element(values.begin(), values.begin() + static_cast<std::ptrdiff_t>(mid));
  return lower + (upper - lower) / 2.0;
}

std::optional<SampleSummary> summarize(std::vector<double> samples) {
  if (samples.empty()) {
    return std::nullopt;
  }
  for (const double s : samples) {
    if (!std::isfinite(s) || s < 0.0) {
      return std::nullopt;
    }
  }
  SampleSummary summary;
  summary.count = samples.size();
  const auto [minIt, maxIt] = std::minmax_element(samples.begin(), samples.end());
  summary.minimum = *minIt;
  summary.maximum = *maxIt;
  summary.median = median(samples);

  std::vector<double> deviations;
  deviations.reserve(samples.size());
  for (const double s : samples) {
    deviations.push_back(std::fabs(s - summary.median));
  }
  summary.mad = median(deviations);
  summary.relativeSpread = summary.median > 0.0 ? summary.mad / summary.median : 0.0;
  return summary;
}

}  // namespace bigolens::benchmark
