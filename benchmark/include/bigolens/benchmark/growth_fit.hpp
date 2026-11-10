#pragma once

#include <optional>
#include <string_view>
#include <vector>

namespace bigolens::benchmark {

enum class GrowthModel { Linear, Linearithmic, Quadratic };

[[nodiscard]] std::string_view toString(GrowthModel model) noexcept;

/// One measured point: input size n and median runtime in milliseconds.
struct Measurement {
  double n = 0.0;
  double milliseconds = 0.0;
};

/// Least-squares fit of t(n) = a + b * f(n) for one model.
struct ModelFit {
  GrowthModel model = GrowthModel::Linear;
  double intercept = 0.0;  ///< a
  double slope = 0.0;      ///< b
  double rss = 0.0;        ///< Residual sum of squares.
  double r2 = 0.0;         ///< Coefficient of determination.
};

struct GrowthFitResult {
  std::vector<ModelFit> fits;           ///< All candidate fits, best first.
  std::optional<GrowthModel> selected;  ///< Empty when the data cannot discriminate.
  bool ambiguous = true;                ///< True if the runner-up fits nearly as well.
};

struct GrowthFitOptions {
  /// Minimum number of distinct input sizes required to fit at all.
  std::size_t minimumPoints = 4;
  /// The best model must have an RSS at most this fraction of the runner-up's.
  double discriminationRatio = 0.5;
};

/// Fits the candidate models. Returns std::nullopt if there are too few points,
/// non-positive sizes or non-finite values. The selected model describes the
/// measured range only and must not be presented as a proof of complexity.
[[nodiscard]] std::optional<GrowthFitResult> fitGrowth(const std::vector<Measurement>& points,
                                                       const GrowthFitOptions& options = {});

/// Predicts t(n) from a fit. Extrapolation beyond the measured range is the
/// caller's responsibility to label as such.
[[nodiscard]] double predict(const ModelFit& fit, double n);

}  // namespace bigolens::benchmark
