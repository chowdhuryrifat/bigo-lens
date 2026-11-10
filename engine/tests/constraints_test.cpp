#include "bigolens/constraints/constraint_set.hpp"

#include <gtest/gtest.h>

using namespace bigolens::constraints;

TEST(ConstraintValidation, ValidSetHasNoProblems) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 10};
  cs.variables["n"] = {1, 200000};
  cs.sums.push_back({"n", "t", 200000});
  cs.timeLimitMs = 2000;
  EXPECT_TRUE(validate(cs).empty());
  ASSERT_NE(findSumConstraint(cs, "n"), nullptr);
  EXPECT_EQ(findSumConstraint(cs, "t"), nullptr);
}

TEST(ConstraintValidation, InvertedRangeIsRejected) {
  ConstraintSet cs;
  cs.variables["n"] = {10, 1};
  EXPECT_FALSE(validate(cs).empty());
}

TEST(ConstraintValidation, SumConstraintNeedsDeclaredBounds) {
  ConstraintSet cs;
  cs.sums.push_back({"n", "t", 100});
  EXPECT_GE(validate(cs).size(), 2U);
}

TEST(ConstraintValidation, ProductAndLinearMustReferenceDeclaredVariables) {
  ConstraintSet cs;
  cs.products.push_back({{"n", "m"}, 1000000});
  cs.linear.push_back({{{"q", 1}}, 10});
  EXPECT_GE(validate(cs).size(), 3U);
}

TEST(ConstraintValidation, MagnitudeLimitIsEnforced) {
  ConstraintSet cs;
  cs.variables["n"] = {0, kMaxConstraintMagnitude + 1};
  EXPECT_FALSE(validate(cs).empty());
}
