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

TEST(Expression, SubstituteReplacesFreeOccurrencesOnly) {
  const auto e = add({variable("hi"), multiply({constant(-1), variable("lo")}), constant(1)});
  const auto shifted = substitute(substitute(e, "lo", constant(0)), "hi", variable("n"));
  EXPECT_EQ(toString(*shifted), "n + 1");
  // A summation index with the same name shadows the substituted variable in its body.
  const auto sum = summation("i", constant(0), variable("i"), variable("i"));
  const auto replaced = substitute(sum, "i", variable("n"));
  EXPECT_EQ(freeVariables(*replaced), std::set<std::string>{"n"});
}
