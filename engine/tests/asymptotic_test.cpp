#include "bigolens/complexity/asymptotic.hpp"

#include <gtest/gtest.h>

using namespace bigolens::complexity;

namespace {
std::string big(const ExprPtr& e) {
  const auto bound = asymptoticUpperBound(*e);
  return bound ? toString(*bound) : "<unsupported>";
}
}  // namespace

TEST(Asymptotic, Linear) {
  EXPECT_EQ(big(variable("n")), "O(n)");
}

TEST(Asymptotic, Constant) {
  EXPECT_EQ(big(constant(42)), "O(1)");
}

TEST(Asymptotic, Product) {
  EXPECT_EQ(big(multiply({variable("n"), variable("m")})), "O(m n)");
}

TEST(Asymptotic, DominatedTermsAreDropped) {
  EXPECT_EQ(big(add({power(variable("n"), 2), variable("n"), constant(5)})), "O(n^2)");
}

TEST(Asymptotic, TriangularSummationIsQuadratic) {
  const auto e = summation("i", constant(0), subtract(variable("n"), constant(1)), variable("i"));
  EXPECT_EQ(big(e), "O(n^2)");
}

TEST(Asymptotic, Logarithmic) {
  EXPECT_EQ(big(add({logarithm(variable("n")), constant(1)})), "O(log n)");
}

TEST(Asymptotic, Linearithmic) {
  EXPECT_EQ(big(multiply({variable("n"), logarithm(variable("n"))})), "O(n log n)");
}

TEST(Asymptotic, IncomparableTermsAreKept) {
  EXPECT_EQ(big(add({power(variable("n"), 2), variable("m")})), "O(n^2 + m)");
}
