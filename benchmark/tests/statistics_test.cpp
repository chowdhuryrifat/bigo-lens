#include "bigolens/benchmark/statistics.hpp"

#include <gtest/gtest.h>

#include <cmath>
#include <limits>

using bigolens::benchmark::summarize;

TEST(Statistics, OddCountMedianAndMad) {
  const auto s = summarize({5.0, 1.0, 3.0, 2.0, 4.0});
  ASSERT_TRUE(s.has_value());
  EXPECT_EQ(s->count, 5U);
  EXPECT_DOUBLE_EQ(s->median, 3.0);
  EXPECT_DOUBLE_EQ(s->minimum, 1.0);
  EXPECT_DOUBLE_EQ(s->maximum, 5.0);
  EXPECT_DOUBLE_EQ(s->mad, 1.0);
  EXPECT_NEAR(s->relativeSpread, 1.0 / 3.0, 1e-12);
}

TEST(Statistics, EvenCountMedianIsMidpoint) {
  const auto s = summarize({4.0, 1.0, 3.0, 2.0});
  ASSERT_TRUE(s.has_value());
  EXPECT_DOUBLE_EQ(s->median, 2.5);
}

TEST(Statistics, OutlierDoesNotMoveMedian) {
  const auto s = summarize({10.0, 10.0, 10.0, 10.0, 1000.0});
  ASSERT_TRUE(s.has_value());
  EXPECT_DOUBLE_EQ(s->median, 10.0);
  EXPECT_DOUBLE_EQ(s->mad, 0.0);
}

TEST(Statistics, RejectsInvalidSamples) {
  EXPECT_FALSE(summarize({}).has_value());
  EXPECT_FALSE(summarize({1.0, -1.0}).has_value());
  EXPECT_FALSE(summarize({1.0, std::numeric_limits<double>::quiet_NaN()}).has_value());
  EXPECT_FALSE(summarize({std::numeric_limits<double>::infinity()}).has_value());
}
