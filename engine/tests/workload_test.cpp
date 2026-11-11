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

TEST(Workload, ExactProductConstraintTightensMonotoneWorkload) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.products.push_back({{"n", "m"}, 1000000});
  const auto r = evaluateWorkload(*multiply({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1e6);
  EXPECT_EQ(r.method, "product-constraint-bound");
  for (const auto& a : r.assumptions) {
    EXPECT_EQ(a.find("not used to tighten"), std::string::npos);
  }
}

TEST(Workload, HigherDegreeProductExpressionBoundedWithSubstitutedMaximum) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.products.push_back({{"n", "m"}, 1000000});
  // n^2 * m: worst case is (n*m) * n <= 1e6 * 1e6 = 1e12.
  const auto r = evaluateWorkload(*multiply({power(variable("n"), 2), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1e12);
  EXPECT_EQ(r.method, "product-constraint-bound");
  bool foundAssumption = false;
  for (const auto& a : r.assumptions) {
    if (a.find("remaining factors evaluated at individual maxima") != std::string::npos) {
      foundAssumption = true;
    }
  }
  EXPECT_TRUE(foundAssumption);
}

TEST(Workload, HigherDegreeProductBothVariables) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.products.push_back({{"n", "m"}, 1000000});
  // n^2 * m^2 = (n*m)^2 <= (1e6)^2 = 1e12.
  const auto r = evaluateWorkload(*multiply({power(variable("n"), 2), power(variable("m"), 2)}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1e12);
  EXPECT_EQ(r.method, "product-constraint-bound");
}

TEST(Workload, NonMonotoneExpressionFallsBackWithAssumption) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.variables["k"] = {1, 1000000};
  cs.products.push_back({{"n", "m"}, 1000000});
  const auto r = evaluateWorkload(*subtract(multiply({variable("n"), variable("m")}), variable("k")), cs);
  EXPECT_EQ(r.status, BoundStatus::Estimated);
  EXPECT_EQ(r.method, "pointwise-maxima");
  bool foundNotUsed = false;
  for (const auto& a : r.assumptions) {
    if (a.find("product constraint (bound 1000000) not used to tighten the bound") != std::string::npos) {
      foundNotUsed = true;
    }
  }
  EXPECT_TRUE(foundNotUsed);
}

TEST(Workload, RedundantProductConstraintRetainsPointwiseMaxima) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 100};
  cs.variables["m"] = {1, 100};
  cs.products.push_back({{"n", "m"}, 1000000});
  const auto r = evaluateWorkload(*multiply({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 10000.0);
  EXPECT_EQ(r.method, "pointwise-maxima");
  for (const auto& a : r.assumptions) {
    EXPECT_EQ(a.find("not used to tighten"), std::string::npos);
  }
}

TEST(Workload, VariablesNotMultipliedTogetherEmitsNotUsedAssumption) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.products.push_back({{"n", "m"}, 1000000});
  const auto r = evaluateWorkload(*add({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 2e6);
  EXPECT_EQ(r.method, "pointwise-maxima");
  bool foundNotUsed = false;
  for (const auto& a : r.assumptions) {
    if (a.find("product constraint (bound 1000000) not used to tighten the bound") != std::string::npos) {
      foundNotUsed = true;
    }
  }
  EXPECT_TRUE(foundNotUsed);
}

TEST(Workload, ProductConstraintWithIndependentFactorAndLogarithm) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1024};
  cs.variables["m"] = {1, 1024};
  cs.variables["k"] = {1, 10};
  cs.products.push_back({{"n", "m"}, 4096});
  // 3 * n * m * k * log2(n) <= 3 * 4096 * 10 * log2(1024) = 3 * 4096 * 10 * 10 = 1228800.
  const auto r = evaluateWorkload(
      *multiply({constant(3.0), variable("n"), variable("m"), variable("k"), logarithm(variable("n"))}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1228800.0);
  EXPECT_EQ(r.method, "product-constraint-bound");
}

TEST(Workload, SuperadditivityRecognition) {
  using constraints::isSuperadditiveIn;
  EXPECT_TRUE(isSuperadditiveIn(*power(variable("n"), 2), "n"));
  EXPECT_TRUE(isSuperadditiveIn(*multiply({variable("n"), logarithm(variable("n"))}), "n"));
  EXPECT_FALSE(isSuperadditiveIn(*logarithm(variable("n")), "n"));
  EXPECT_FALSE(isSuperadditiveIn(*add({variable("n"), constant(1)}), "n"));
}

TEST(Workload, ExactLinearConstraintTightensProduct) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.linear.push_back({{{"n", 1}, {"m", 1}}, 200000});
  const auto r = evaluateWorkload(*multiply({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 1e10);
  EXPECT_EQ(r.method, "linear-constraint-bound");
  for (const auto& a : r.assumptions) {
    EXPECT_EQ(a.find("not used to tighten"), std::string::npos);
  }
}

TEST(Workload, DegreeMismatchedProductUnderLinearConstraint) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.linear.push_back({{{"n", 1}, {"m", 1}}, 300000});
  // n^2 * m: peak is at n = 2/3 * 3e5 = 2e5, m = 1e5 => (2e5)^2 * 1e5 = 4e15.
  const auto r = evaluateWorkload(*multiply({power(variable("n"), 2), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 4e15);
  EXPECT_EQ(r.method, "linear-constraint-bound");
}

TEST(Workload, LinearSumTightenedDirectly) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.linear.push_back({{{"n", 1}, {"m", 1}}, 200000});
  const auto r = evaluateWorkload(*add({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 200000.0);
  EXPECT_EQ(r.method, "linear-constraint-bound");
}

TEST(Workload, VariableBoundTighteningFromLinearConstraint) {
  ConstraintSet cs;
  cs.variables["n"] = {0, 1000000};
  cs.variables["m"] = {0, 1000000};
  cs.linear.push_back({{{"n", 1}, {"m", 1}}, 200000});
  // n <= 2e5 - 0 = 2e5.
  const auto r = evaluateWorkload(*power(variable("n"), 2), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 4e10);
  EXPECT_EQ(r.method, "linear-constraint-bound");
}

TEST(Workload, LinearConstraintNegativeCoefficientEmitsAssumption) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 100};
  cs.variables["m"] = {1, 100};
  cs.linear.push_back({{{"n", 1}, {"m", -1}}, 10});
  const auto r = evaluateWorkload(*multiply({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 10000.0);
  EXPECT_EQ(r.method, "pointwise-maxima");
  bool foundNegAssumption = false;
  for (const auto& a : r.assumptions) {
    if (a.find("linear constraint with negative coefficient (bound 10) not used to tighten the bound") !=
        std::string::npos) {
      foundNegAssumption = true;
    }
  }
  EXPECT_TRUE(foundNegAssumption);
}

TEST(Workload, LinearConstraintNonMonotoneExpressionFallsBackWithAssumption) {
  ConstraintSet cs;
  cs.variables["n"] = {1, 1000000};
  cs.variables["m"] = {1, 1000000};
  cs.variables["k"] = {1, 1000000};
  cs.linear.push_back({{{"n", 1}, {"m", 1}}, 200000});
  const auto r = evaluateWorkload(*subtract(multiply({variable("n"), variable("m")}), variable("k")), cs);
  EXPECT_EQ(r.status, BoundStatus::Estimated);
  EXPECT_EQ(r.method, "pointwise-maxima");
  bool foundAssumption = false;
  for (const auto& a : r.assumptions) {
    if (a.find("linear constraint (bound 200000) not used to tighten the bound on non-monotone expression") !=
        std::string::npos) {
      foundAssumption = true;
    }
  }
  EXPECT_TRUE(foundAssumption);
}

TEST(Workload, LinearConstraintPropertyTestSoundnessAgainstBruteForce) {
  struct TestCase {
    int c1;
    int c2;
    int bound;
    int maxN;
    int maxM;
  };

  const std::vector<TestCase> cases = {
      {1, 1, 20, 25, 25},
      {1, 2, 30, 20, 20},
      {2, 3, 40, 25, 25},
      {3, 1, 25, 15, 30},
  };

  for (const auto& tc : cases) {
    ConstraintSet cs;
    cs.variables["n"] = {1, tc.maxN};
    cs.variables["m"] = {1, tc.maxM};
    cs.linear.push_back({{{"n", tc.c1}, {"m", tc.c2}}, tc.bound});

    const std::vector<std::pair<ExprPtr, std::function<double(int, int)>>> exprs = {
        {multiply({variable("n"), variable("m")}), [](int n, int m) { return static_cast<double>(n * m); }},
        {multiply({power(variable("n"), 2), variable("m")}),
         [](int n, int m) { return static_cast<double>(n * n * m); }},
        {multiply({variable("n"), power(variable("m"), 2)}),
         [](int n, int m) { return static_cast<double>(n * m * m); }},
        {add({variable("n"), variable("m")}), [](int n, int m) { return static_cast<double>(n + m); }},
        {add({multiply({constant(2.0), variable("n")}), multiply({constant(3.0), variable("m")})}),
         [](int n, int m) { return static_cast<double>(2 * n + 3 * m); }},
    };

    for (const auto& [expr, evalFn] : exprs) {
      double bruteForceMax = 0.0;
      bool foundValid = false;
      for (int n = 1; n <= tc.maxN; ++n) {
        for (int m = 1; m <= tc.maxM; ++m) {
          if (tc.c1 * n + tc.c2 * m <= tc.bound) {
            foundValid = true;
            double val = evalFn(n, m);
            if (val > bruteForceMax) {
              bruteForceMax = val;
            }
          }
        }
      }
      if (!foundValid) {
        continue;
      }

      const auto r = evaluateWorkload(*expr, cs);
      EXPECT_EQ(r.status, BoundStatus::Proven);
      ASSERT_TRUE(r.upperBound.has_value());
      EXPECT_GE(*r.upperBound, bruteForceMax - 1e-6)
          << "Soundness violated for tc (" << tc.c1 << "n + " << tc.c2 << "m <= " << tc.bound << ") maxN=" << tc.maxN
          << " maxM=" << tc.maxM << ": upperBound=" << *r.upperBound << " bruteForceMax=" << bruteForceMax;
    }
  }
}

TEST(Workload, MultipleSumConstraintsSeparableLinearSum) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 10000};
  cs.variables["n"] = {1, 200000};
  cs.variables["m"] = {1, 200000};
  cs.sums.push_back({"n", "t", 200000});
  cs.sums.push_back({"m", "t", 300000});

  const auto r = evaluateWorkload(*multiply({variable("t"), add({variable("n"), variable("m")})}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 500000.0);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}

TEST(Workload, MultipleSumConstraintsSeparableWithHigherPowers) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 100};
  cs.variables["n"] = {1, 1000};
  cs.variables["m"] = {1, 2000};
  cs.sums.push_back({"n", "t", 5000});
  cs.sums.push_back({"m", "t", 4000});

  // t * (n^2 + m): n^2 packs as 5 * 1000^2 = 5e6. m is linear: 4000. Total = 5004000.
  const auto r = evaluateWorkload(*multiply({variable("t"), add({power(variable("n"), 2), variable("m")})}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 5004000.0);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}

TEST(Workload, MultipleSumConstraintsNonSeparableProductFallback) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 10};
  cs.variables["n"] = {1, 100};
  cs.variables["m"] = {1, 100};
  cs.sums.push_back({"n", "t", 200});
  cs.sums.push_back({"m", "t", 200});

  // t * (n * m): non-separable product across two sum-constrained variables.
  const auto r = evaluateWorkload(*multiply({variable("t"), multiply({variable("n"), variable("m")})}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  // cases = min(10, 200/1, 200/1) = 10; perCase = 100 * 100 = 10000; total = 100000.
  EXPECT_DOUBLE_EQ(*r.upperBound, 100000.0);
  EXPECT_EQ(r.method, "sum-constraint-cases");
}

TEST(Workload, MultipleSumConstraintsDifferentCountVariablesGivesUnknown) {
  ConstraintSet cs;
  cs.variables["t1"] = {1, 100};
  cs.variables["t2"] = {1, 100};
  cs.variables["n"] = {1, 1000};
  cs.variables["m"] = {1, 1000};
  cs.sums.push_back({"n", "t1", 5000});
  cs.sums.push_back({"m", "t2", 5000});

  const auto r = evaluateWorkload(*add({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Unknown);
  EXPECT_FALSE(r.upperBound.has_value());
  bool foundAssumption = false;
  for (const auto& a : r.assumptions) {
    if (a.find("sum constraints over different test-case counts are not supported") != std::string::npos) {
      foundAssumption = true;
    }
  }
  EXPECT_TRUE(foundAssumption);
}

TEST(Workload, MultipleSumConstraintsSeparableWithoutExplicitCount) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 10000};
  cs.variables["n"] = {1, 200000};
  cs.variables["m"] = {1, 200000};
  cs.sums.push_back({"n", "t", 200000});
  cs.sums.push_back({"m", "t", 300000});

  // n + m treated as per-case: 200000 + 300000 = 500000.
  const auto r = evaluateWorkload(*add({variable("n"), variable("m")}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 500000.0);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}

TEST(Workload, MultipleSumConstraintsSeparableWithIndependentTerm) {
  ConstraintSet cs;
  cs.variables["t"] = {1, 100};
  cs.variables["n"] = {1, 1000};
  cs.variables["m"] = {1, 2000};
  cs.sums.push_back({"n", "t", 1000});
  cs.sums.push_back({"m", "t", 2000});

  // t * (n + m + 5) = 1000 + 2000 + 100 * 5 = 3500.
  const auto r = evaluateWorkload(*multiply({variable("t"), add({variable("n"), variable("m"), constant(5.0)})}), cs);
  EXPECT_EQ(r.status, BoundStatus::Proven);
  ASSERT_TRUE(r.upperBound.has_value());
  EXPECT_DOUBLE_EQ(*r.upperBound, 3500.0);
  EXPECT_EQ(r.method, "sum-constraint-packing");
}
