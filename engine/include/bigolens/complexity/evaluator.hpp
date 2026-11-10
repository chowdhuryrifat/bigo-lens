#pragma once

#include <cstdint>
#include <map>
#include <string>

#include "bigolens/complexity/expression.hpp"

namespace bigolens::complexity {

using Bindings = std::map<std::string, double, std::less<>>;

enum class EvalStatus { Ok, UnboundVariable, Overflow, DomainError, BudgetExceeded };

struct EvalResult {
  EvalStatus status = EvalStatus::Ok;
  double value = 0.0;
  std::string detail;  ///< Explanation when status != Ok.

  [[nodiscard]] bool ok() const noexcept { return status == EvalStatus::Ok; }
};

struct EvalOptions {
  /// Maximum number of node evaluations. Summations whose body depends on the
  /// index in a non-affine way are evaluated term by term and consume budget.
  std::uint64_t budget = 50'000'000;
};

/// Numerically evaluates an expression. Arithmetic is IEEE double; any
/// non-finite intermediate result is reported as Overflow rather than returned.
/// Values above 2^53 are approximate.
[[nodiscard]] EvalResult evaluate(const Expr& expr, const Bindings& bindings, const EvalOptions& options = {});

}  // namespace bigolens::complexity
