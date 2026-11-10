#include "bigolens/constraints/constraint_set.hpp"

#include <cstdlib>

namespace bigolens::constraints {
namespace {

bool inRange(std::int64_t value) {
  return value >= -kMaxConstraintMagnitude && value <= kMaxConstraintMagnitude;
}

bool hasUpperBound(const ConstraintSet& cs, const std::string& name) {
  const auto it = cs.variables.find(name);
  return it != cs.variables.end() && it->second.max.has_value();
}

}  // namespace

std::vector<std::string> validate(const ConstraintSet& cs) {
  std::vector<std::string> problems;
  for (const auto& [name, bound] : cs.variables) {
    if ((bound.min && !inRange(*bound.min)) || (bound.max && !inRange(*bound.max))) {
      problems.push_back("bounds of '" + name + "' exceed the supported magnitude 10^18");
    }
    if (bound.min && bound.max && *bound.min > *bound.max) {
      problems.push_back("variable '" + name + "' has min greater than max");
    }
  }
  for (const auto& sum : cs.sums) {
    if (!hasUpperBound(cs, sum.variable)) {
      problems.push_back("sum constraint references '" + sum.variable + "', which has no declared upper bound");
    }
    if (!hasUpperBound(cs, sum.countVariable)) {
      problems.push_back("sum constraint count variable '" + sum.countVariable + "' has no declared upper bound");
    }
    if (sum.variable == sum.countVariable) {
      problems.push_back("sum constraint variable and count variable must differ");
    }
    if (sum.maxTotal < 0 || !inRange(sum.maxTotal)) {
      problems.push_back("sum constraint total for '" + sum.variable + "' must be in [0, 10^18]");
    }
  }
  for (std::size_t i = 0; i < cs.sums.size(); ++i) {
    for (std::size_t j = i + 1; j < cs.sums.size(); ++j) {
      if (cs.sums[i].variable == cs.sums[j].variable) {
        problems.push_back("multiple sum constraints for '" + cs.sums[i].variable + "'");
      }
    }
  }
  for (const auto& product : cs.products) {
    if (product.variables.size() < 2) {
      problems.push_back("product constraint needs at least two variables");
    }
    for (const auto& name : product.variables) {
      if (cs.variables.find(name) == cs.variables.end()) {
        problems.push_back("product constraint references undeclared variable '" + name + "'");
      }
    }
    if (product.maxProduct < 0 || !inRange(product.maxProduct)) {
      problems.push_back("product constraint bound must be in [0, 10^18]");
    }
  }
  for (const auto& linear : cs.linear) {
    if (linear.coefficients.empty()) {
      problems.push_back("linear constraint needs at least one term");
    }
    for (const auto& [name, coefficient] : linear.coefficients) {
      if (cs.variables.find(name) == cs.variables.end()) {
        problems.push_back("linear constraint references undeclared variable '" + name + "'");
      }
      if (!inRange(coefficient)) {
        problems.push_back("linear constraint coefficient exceeds 10^18");
      }
    }
    if (!inRange(linear.bound)) {
      problems.push_back("linear constraint bound exceeds 10^18");
    }
  }
  if (cs.timeLimitMs && (*cs.timeLimitMs <= 0 || *cs.timeLimitMs > 3'600'000)) {
    problems.push_back("time limit must be in (0, 3600000] ms");
  }
  if (cs.memoryLimitBytes && (*cs.memoryLimitBytes <= 0 || !inRange(*cs.memoryLimitBytes))) {
    problems.push_back("memory limit must be positive");
  }
  return problems;
}

const SumConstraint* findSumConstraint(const ConstraintSet& cs, const std::string& variable) {
  for (const auto& sum : cs.sums) {
    if (sum.variable == variable) {
      return &sum;
    }
  }
  return nullptr;
}

}  // namespace bigolens::constraints
