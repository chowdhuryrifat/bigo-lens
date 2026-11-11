#include "bigolens/estimation/stl_cost_model.hpp"

#include <gtest/gtest.h>

#include "bigolens/complexity/asymptotic.hpp"

using namespace bigolens;

namespace {
std::string big(const complexity::ExprPtr& e) {
  return complexity::toString(*complexity::asymptoticUpperBound(*e));
}
}  // namespace

TEST(StlCostModel, UnorderedMapDistinguishesAverageAndWorstCase) {
  const auto cost = estimation::lookupStlCost("std::unordered_map", "find");
  ASSERT_TRUE(cost.has_value());
  EXPECT_EQ(big(cost->average), "O(1)");
  EXPECT_EQ(big(cost->worst), "O(n)");
}

TEST(StlCostModel, VectorPushBackIsAmortized) {
  const auto cost = estimation::lookupStlCost("std::vector", "push_back");
  ASSERT_TRUE(cost.has_value());
  EXPECT_TRUE(cost->amortized);
  EXPECT_EQ(big(cost->worst), "O(n)");
}

TEST(StlCostModel, SortIsLinearithmic) {
  const auto cost = estimation::lookupStlCost("algorithm", "sort");
  ASSERT_TRUE(cost.has_value());
  EXPECT_EQ(big(cost->worst), "O(n log n)");
}

TEST(StlCostModel, RequiredOperationsArePresent) {
  for (const auto& [container, op] : std::vector<std::pair<const char*, const char*>>{{"std::vector", "operator[]"},
                                                                                      {"std::vector", "erase"},
                                                                                      {"std::map", "find"},
                                                                                      {"std::set", "insert"},
                                                                                      {"std::priority_queue", "push"},
                                                                                      {"std::priority_queue", "pop"}}) {
    EXPECT_TRUE(estimation::lookupStlCost(container, op).has_value()) << container << "::" << op;
  }
  EXPECT_FALSE(estimation::lookupStlCost("std::list", "sort").has_value());
}

TEST(StlCostModel, SizeVariableBindingSubstitutesConstraintVariables) {
  const auto cost = estimation::lookupStlCost("std::set", "insert");
  ASSERT_TRUE(cost.has_value());
  const auto expr1 = complexity::substitute(cost->worst, "n", complexity::variable("n"));
  EXPECT_EQ(complexity::toString(*expr1), "log_2(n)");

  const auto expr2 = complexity::substitute(cost->worst, "n", complexity::variable("s.size()"));
  EXPECT_EQ(complexity::toString(*expr2), "log_2(s.size())");

  const auto expr3 = complexity::substitute(cost->worst, "n", complexity::variable("m"));
  EXPECT_EQ(complexity::toString(*expr3), "log_2(m)");

  const auto sortCost = estimation::lookupStlCost("algorithm", "sort");
  ASSERT_TRUE(sortCost.has_value());
  const auto expr4 = complexity::substitute(sortCost->worst, "n", complexity::variable("n"));
  EXPECT_EQ(complexity::toString(*expr4), "n * log_2(n)");
}
