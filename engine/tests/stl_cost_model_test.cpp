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
