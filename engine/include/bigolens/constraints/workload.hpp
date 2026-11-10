#pragma once

#include <optional>
#include <string>
#include <vector>

#include "bigolens/complexity/expression.hpp"
#include "bigolens/constraints/constraint_set.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::constraints {

/// Worst-case structural workload of an expression under constraints.
struct WorkloadResult {
  models::BoundStatus status = models::BoundStatus::Unknown;
  std::optional<double> upperBound;  ///< Present when status is Proven or Estimated.
  std::string method;                ///< e.g. "pointwise-maxima", "sum-constraint-packing".
  std::vector<std::string> assumptions;
  std::vector<std::string> missing;  ///< Variables lacking an upper bound.
};

/// Computes an upper bound of `expr` under `constraints`.
///
/// - Every free variable needs an upper bound, otherwise the status is Unknown.
/// - If the expression is syntactically monotone non-decreasing, evaluating at
///   the variable maxima is a sound upper bound (Proven relative to the
///   expression); otherwise the result is Estimated.
/// - If a variable is subject to a sum constraint, the expression is treated as
///   a per-test-case cost and aggregated across test cases. For expressions
///   recognised as superadditive (e.g. n^2, n log n, n(n-1)/2) the exact
///   maximum is obtained by packing full-size cases; otherwise a looser but
///   sound bound (cases x per-case maximum) is used.
[[nodiscard]] WorkloadResult evaluateWorkload(const complexity::Expr& expr, const ConstraintSet& constraints);

/// Constraints for sizing one live instance of a buffer: every sum-constrained
/// variable is capped at min(max, total) and the sum relations are removed, so
/// per-test-case allocations are not added up across test cases.
[[nodiscard]] ConstraintSet singleInstanceConstraints(const ConstraintSet& constraints);

/// True if `expr` is syntactically non-decreasing in every free variable, given
/// that variables are >= 0.
[[nodiscard]] bool isMonotoneNonDecreasing(const complexity::Expr& expr);

/// True if `expr`, as a function of `variable` (others fixed and >= 0), is
/// recognised as non-negative, non-decreasing and superadditive.
[[nodiscard]] bool isSuperadditiveIn(const complexity::Expr& expr, const std::string& variable);

}  // namespace bigolens::constraints
