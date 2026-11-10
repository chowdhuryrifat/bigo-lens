#include "bigolens/benchmark/growth_fit.hpp"

#include <gtest/gtest.h>

#include <cmath>
#include <vector>

using namespace bigolens::benchmark;

namespace {
std::vector<Measurement> synthetic(double (*f)(double)) {
  std::vector<Measurement> points;
  for (double n = 1000; n <= 64000; n *= 2) {
    points.push_back({n, 0.5 + f(n)});
  }
  return points;
}
}  // namespace

TEST(GrowthFit, SelectsQuadraticForQuadraticData) {
  const auto result = fitGrowth(synthetic([](double n) { return 1e-6 * n * n; }));
  ASSERT_TRUE(result.has_value());
  ASSERT_TRUE(result->selected.has_value());
  EXPECT_EQ(*result->selected, GrowthModel::Quadratic);
  EXPECT_FALSE(result->ambiguous);
  EXPECT_NEAR(result->fits.front().slope, 1e-6, 1e-9);
}

TEST(GrowthFit, SelectsLinearForLinearData) {
  const auto result = fitGrowth(synthetic([](double n) { return 2e-3 * n; }));
  ASSERT_TRUE(result.has_value());
  ASSERT_TRUE(result->selected.has_value());
  EXPECT_EQ(*result->selected, GrowthModel::Linear);
}

TEST(GrowthFit, SelectsLinearithmicForNLogNData) {
  const auto result = fitGrowth(synthetic([](double n) { return 1e-4 * n * std::log2(n); }));
  ASSERT_TRUE(result.has_value());
  ASSERT_TRUE(result->selected.has_value());
  EXPECT_EQ(*result->selected, GrowthModel::Linearithmic);
}

TEST(GrowthFit, ConstantDataIsAmbiguous) {
  std::vector<Measurement> points;
  for (double n = 1000; n <= 64000; n *= 2) {
    points.push_back({n, 3.0});
  }
  const auto result = fitGrowth(points);
  ASSERT_TRUE(result.has_value());
  EXPECT_TRUE(result->ambiguous);
  EXPECT_FALSE(result->selected.has_value());
}

TEST(GrowthFit, RejectsTooFewOrInvalidPoints) {
  EXPECT_FALSE(fitGrowth({{1, 1}, {2, 2}, {3, 3}}).has_value());
  EXPECT_FALSE(fitGrowth({{1, 1}, {2, 2}, {3, 3}, {-4, 4}}).has_value());
  EXPECT_FALSE(fitGrowth({{1, 1}, {1, 2}, {1, 3}, {1, 4}}).has_value());
}
