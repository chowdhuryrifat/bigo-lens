#pragma once

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <vector>

namespace bigolens::constraints {

/// Largest magnitude accepted for any constraint value (10^18).
inline constexpr std::int64_t kMaxConstraintMagnitude = 1'000'000'000'000'000'000;

/// lo <= variable <= hi. Either side may be absent.
struct VariableBound {
  std::optional<std::int64_t> min;
  std::optional<std::int64_t> max;
};

/// Over all test cases (whose count is `countVariable`), the sum of
/// `variable` is at most `maxTotal`. Example: sum of N over T cases <= 2e5.
struct SumConstraint {
  std::string variable;
  std::string countVariable;
  std::int64_t maxTotal = 0;
};

/// The product of `variables` is at most `maxProduct`. Example: N*M <= 1e6.
struct ProductConstraint {
  std::vector<std::string> variables;
  std::int64_t maxProduct = 0;
};

/// sum(coefficient_i * variable_i) <= bound.
struct LinearConstraint {
  std::map<std::string, std::int64_t> coefficients;
  std::int64_t bound = 0;
};

struct ConstraintSet {
  std::map<std::string, VariableBound, std::less<>> variables;
  std::vector<SumConstraint> sums;
  std::vector<ProductConstraint> products;
  std::vector<LinearConstraint> linear;
  std::optional<std::int64_t> timeLimitMs;
  std::optional<std::int64_t> memoryLimitBytes;
};

/// Returns human-readable problems (empty when valid): inverted ranges,
/// out-of-range magnitudes, references to undeclared variables, etc.
[[nodiscard]] std::vector<std::string> validate(const ConstraintSet& constraints);

/// Returns the sum constraint applying to `variable`, if any.
[[nodiscard]] const SumConstraint* findSumConstraint(const ConstraintSet& constraints, const std::string& variable);

}  // namespace bigolens::constraints
