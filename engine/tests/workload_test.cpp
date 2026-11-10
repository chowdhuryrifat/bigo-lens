#include "bigolens/constraints/workload.hpp"

#include <gtest/gtest.h>

#include "bigolens/complexity/expression.hpp"

using namespace bigolens;
using namespace bigolens::complexity;
using bigolens::constraints::ConstraintSet;
using bigolens::constraints::evaluateWorkload;
using bigolens::models::BoundStatus;

namespace {
ConstraintSet multiTestCase() {
  ConstraintSet cs;
  cs.variables["t"] = {1, 10};
  cs.variables["n"] = {1, 200000};
  cs.sums.push_back({"n", "t", 200000});
  return cs;
}
}  // namespace

TEST(Workload, PointwiseMaximaForMonotoneExpression) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000};
  const auto r = evaluateWorkload(*power(variable("n"), 2), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1e6);
  EXPECT_EQ(r.method, "pointwise-maxima");
}

TEST(Workload, MissingBoundGivesUnknown) {
  const auto r = evaluateWorkload(*multiply({variable("n"), variable("m")}), ConstraintSet{});
  EXPECT_EQ(r.status, BoundStatus::Unknown);
  EXPECT_FALSE(r.upperBound.has_value());
  EXPECT_EQ(r.missing.size(), 2U);
}

TEST(Workload, SumConstraintPreventsTTimesNSquared) {
  // t * n^2 with t <= 10, n <= 2e5, sum n <= 2e5: worst case is one case of size 2e5.
  const auto e = multiply({variable("t"), power(variable("n"), 2)});
  const auto r = evaluateWorkload(*e, multiTestCase());
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 4e10);
  EXPECT_LT(*r.upperBound, 10.0 * 4e10);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}

TEST(Workload, SumConstraintPackingWithSmallPerCaseMaximum) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 100};
  cs.variables["n"] = {1, 1000};
  cs.sums.push_back({"n", "t", 5500});
  // 5 full cases of 1000 plus one of 500: 5 * 1e6 + 2.5e5.
  const auto r = evaluateWorkload(*multiply({variable("t"), power(variable("n"), 2)}), cs);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 5e6 + 2.5e5);
}

TEST(Workload, LinearPerCaseCostUnderSumConstraintIsTheTotal) {
  const auto r = evaluateWorkload(*multiply({variable("t"), variable("n")}), multiTestCase());
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 200000.0);
}

TEST(Workload, NonMonotoneExpressionIsOnlyEstimated) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 100};
  cs.variables["k"] = {1, 100};
  const auto r = evaluateWorkload(*subtract(variable("n"), variable("k")), cs);
  EXPECT_EQ(r.status, BoundStatus::Estimated);
}

TEST(Workload, SuperadditivityRecognition) {
  using constraints::isSuperadditiveIn;
  EXPECT_TRUE(isSuperadditiveIn(*power(variable("n"), 2), "n"));
  EXPECT_TRUE(isSuperadditiveIn(*multiply({variable("n"), logarithm(variable("n"))}), "n"));
  EXPECT_FALSE(isSuperadditiveIn(*logarithm(variable("n")), "n"));
  EXPECT_FALSE(isSuperadditiveIn(*add({variable("n"), constant(1)}), "n"));
}
