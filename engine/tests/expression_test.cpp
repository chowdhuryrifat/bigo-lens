#include "bigolens/complexity/expression.hpp"

#include <gtest/gtest.h>

#include <stdexcept>

using namespace bigolens::complexity;

TEST(Expression, ConstantFoldingInAddAndMultiply) {
  EXPECT_TRUE(isConstant(*add({constant(2), constant(3)}), 5.0));
  EXPECT_TRUE(isConstant(*multiply({constant(2), constant(3)}), 6.0));
  EXPECT_TRUE(isConstant(*multiply({variable("n"), constant(0)}), 0.0));
}

TEST(Expression, IdentityElementsAreDropped) {
  EXPECT_EQ(toString(*add({variable("n"), constant(0)})), "n");
  EXPECT_EQ(toString(*multiply({variable("n"), constant(1)})), "n");
}

TEST(Expression, FreeVariablesExcludeSummationIndex) {
  const auto sum = summation("i", constant(0), subtract(variable("n"), constant(1)), variable("i"));
  const auto free = freeVariables(*sum);
  EXPECT_EQ(free.size(), 1U);
  EXPECT_EQ(free.count("n"), 1U);
  EXPECT_EQ(free.count("i"), 0U);
}

TEST(Expression, InvalidPowerAndLogarithmThrow) {
  EXPECT_THROW((void)power(variable("n"), -1.0), std::invalid_argument);
  EXPECT_THROW((void)logarithm(variable("n"), 1.0), std::invalid_argument);
  EXPECT_THROW((void)logarithm(variable("n"), 0.5), std::invalid_argument);
}

TEST(Expression, AllKindsRender) {
  const auto n = variable("n");
  EXPECT_FALSE(toString(*power(n, 2)).empty());
  EXPECT_FALSE(toString(*logarithm(n)).empty());
  EXPECT_FALSE(toString(*maximum({n, variable("m")})).empty());
  EXPECT_NE(toString(*summation("i", constant(0), n, variable("i"))).find("sum"), std::string::npos);
}
