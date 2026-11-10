#include "bigolens/complexity/evaluator.hpp"

#include <gtest/gtest.h>

#include <cmath>

using namespace bigolens::complexity;

TEST(Evaluator, EvaluatesArithmetic) {
  const auto e = add({multiply({variable("n"), variable("m")}), constant(3)});
  const auto r = evaluate(*e, {{"n", 10}, {"m", 20}});
  ASSERT_TRUE(r.ok());
  EXPECT_DOUBLE_EQ(r.value, 203.0);
}

TEST(Evaluator, UnboundVariableIsReported) {
  const auto r = evaluate(*variable("n"), {});
  EXPECT_EQ(r.status, EvalStatus::UnboundVariable);
  EXPECT_FALSE(r.detail.empty());
}

TEST(Evaluator, TriangularSummationMatchesClosedForm) {
  // sum_{i=0}^{n-1} i = n(n-1)/2
  const auto e = summation("i", constant(0), subtract(variable("n"), constant(1)), variable("i"));
  const auto r = evaluate(*e, {{"n", 200000}});
  ASSERT_TRUE(r.ok());
  EXPECT_DOUBLE_EQ(r.value, 200000.0 * 199999.0 / 2.0);
}

TEST(Evaluator, EmptySummationIsZero) {
  const auto e = summation("i", constant(5), constant(4), constant(1));
  const auto r = evaluate(*e, {});
  ASSERT_TRUE(r.ok());
  EXPECT_DOUBLE_EQ(r.value, 0.0);
}

TEST(Evaluator, LogarithmClampsArgumentToOne) {
  const auto r0 = evaluate(*logarithm(variable("n")), {{"n", 0}});
  ASSERT_TRUE(r0.ok());
  EXPECT_DOUBLE_EQ(r0.value, 0.0);
  const auto r = evaluate(*logarithm(variable("n")), {{"n", 1024}});
  ASSERT_TRUE(r.ok());
  EXPECT_DOUBLE_EQ(r.value, 10.0);
}

TEST(Evaluator, OverflowIsReportedNotReturned) {
  const auto e = power(variable("n"), 400);
  const auto r = evaluate(*e, {{"n", 1e18}});
  EXPECT_EQ(r.status, EvalStatus::Overflow);
}

TEST(Evaluator, NonAffineSummationRespectsBudget) {
  // Body i^2 is not affine in i; term-by-term evaluation must stop at the budget.
  const auto e = summation("i", constant(0), variable("n"), power(variable("i"), 2));
  EvalOptions options;
  options.budget = 1000;
  const auto r = evaluate(*e, {{"n", 1e9}}, options);
  EXPECT_EQ(r.status, EvalStatus::BudgetExceeded);
}

TEST(Evaluator, MaximumPicksLargest) {
  const auto r = evaluate(*maximum({variable("a"), variable("b")}), {{"a", 3}, {"b", 7}});
  ASSERT_TRUE(r.ok());
  EXPECT_DOUBLE_EQ(r.value, 7.0);
}
