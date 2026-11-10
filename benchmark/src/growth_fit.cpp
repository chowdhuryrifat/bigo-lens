#include "bigolens/benchmark/growth_fit.hpp"

#include <algorithm>
#include <cmath>
#include <set>

namespace bigolens::benchmark {
namespace {

double feature(GrowthModel model, double n) {
  switch (model) {
    case GrowthModel::Linear:
      return n;
    case GrowthModel::Linearithmic:
      return n * std::log2(std::max(n, 2.0));
    case GrowthModel::Quadratic:
      return n * n;
  }
  return n;
}

ModelFit fitOne(GrowthModel model, const std::vector<Measurement>& points) {
  // Ordinary least squares for t = a + b x, x = f(n). Features are scaled to
  // keep the normal equations well conditioned.
  double maxX = 0.0;
  for (const auto& p : points) {
    maxX = std::max(maxX, feature(model, p.n));
  }
  const double scale = maxX > 0.0 ? maxX : 1.0;
  const double count = static_cast<double>(points.size());
  double sumX = 0.0;
  double sumY = 0.0;
  for (const auto& p : points) {
    sumX += feature(model, p.n) / scale;
    sumY += p.milliseconds;
  }
  const double meanX = sumX / count;
  const double meanY = sumY / count;
  double sxx = 0.0;
  double sxy = 0.0;
  double syy = 0.0;
  for (const auto& p : points) {
    const double dx = feature(model, p.n) / scale - meanX;
    const double dy = p.milliseconds - meanY;
    sxx += dx * dx;
    sxy += dx * dy;
    syy += dy * dy;
  }
  ModelFit fit;
  fit.model = model;
  const double scaledSlope = sxx > 0.0 ? sxy / sxx : 0.0;
  fit.slope = scaledSlope / scale;
  fit.intercept = meanY - scaledSlope * meanX;
  for (const auto& p : points) {
    const double residual = p.milliseconds - predict(fit, p.n);
    fit.rss += residual * residual;
  }
  fit.r2 = syy > 0.0 ? 1.0 - fit.rss / syy : 0.0;
  return fit;
}

}  // namespace

std::string_view toString(GrowthModel model) noexcept {
  switch (model) {
    case GrowthModel::Linear:
      return "linear";
    case GrowthModel::Linearithmic:
      return "linearithmic";
    case GrowthModel::Quadratic:
      return "quadratic";
  }
  return "linear";
}

double predict(const ModelFit& fit, double n) {
  return fit.intercept + fit.slope * feature(fit.model, n);
}

std::optional<GrowthFitResult> fitGrowth(const std::vector<Measurement>& points, const GrowthFitOptions& options) {
  std::set<double> sizes;
  for (const auto& p : points) {
    if (!std::isfinite(p.n) || !std::isfinite(p.milliseconds) || p.n <= 0.0 || p.milliseconds < 0.0) {
      return std::nullopt;
    }
    sizes.insert(p.n);
  }
  if (sizes.size() < std::max<std::size_t>(options.minimumPoints, 3)) {
    return std::nullopt;
  }

  GrowthFitResult result;
  for (const auto model : {GrowthModel::Linear, GrowthModel::Linearithmic, GrowthModel::Quadratic}) {
    ModelFit fit = fitOne(model, points);
    // A negative slope means runtime decreases with n: physically implausible for these models.
    if (fit.slope >= 0.0) {
      result.fits.push_back(fit);
    }
  }
  std::sort(result.fits.begin(), result.fits.end(), [](const ModelFit& a, const ModelFit& b) { return a.rss < b.rss; });
  if (result.fits.empty()) {
    return result;
  }
  // Flat data (no measurable growth) or tied residuals cannot discriminate between models.
  const bool grows = result.fits[0].slope > 0.0;
  const bool discriminates =
      grows && (result.fits.size() == 1 || result.fits[0].rss < options.discriminationRatio * result.fits[1].rss);
  result.ambiguous = !discriminates;
  if (discriminates) {
    result.selected = result.fits[0].model;
  }
  return result;
}

}  // namespace bigolens::benchmark
