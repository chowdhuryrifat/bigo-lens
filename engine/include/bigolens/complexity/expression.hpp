#pragma once

#include <memory>
#include <set>
#include <string>
#include <vector>

namespace bigolens::complexity {

enum class ExprKind { Constant, Variable, Add, Multiply, Power, Logarithm, Summation, Maximum };

class Expr;
/// Expressions are immutable and may share sub-trees.
using ExprPtr = std::shared_ptr<const Expr>;

/// Node of a symbolic cost expression.
///
/// Semantics (all variables are integers that are >= 0 unless constrained):
///  - Constant(c)                    c
///  - Variable(v)                    value bound to v
///  - Add(a, b, ...)                 a + b + ...
///  - Multiply(a, b, ...)            a * b * ...
///  - Power(a, k)                    a^k, k a non-negative constant
///  - Logarithm(a, base)             log_base(max(a, 1))
///  - Summation(i, lo, hi, body)     sum of body for integer i in [lo, hi]
///  - Maximum(a, b, ...)             max(a, b, ...)
///
/// Construct nodes with the factory functions below, which apply only
/// semantics-preserving simplifications.
class Expr {
 public:
  [[nodiscard]] ExprKind kind() const noexcept { return kind_; }
  /// Constant value, Power exponent, or Logarithm base.
  [[nodiscard]] double number() const noexcept { return number_; }
  /// Variable name, or the Summation index name.
  [[nodiscard]] const std::string& name() const noexcept { return name_; }
  /// Children: operands of Add/Multiply/Maximum, the base of Power/Logarithm,
  /// or {lower, upper, body} for Summation.
  [[nodiscard]] const std::vector<ExprPtr>& operands() const noexcept { return operands_; }

  Expr(ExprKind kind, double number, std::string name, std::vector<ExprPtr> operands);

 private:
  ExprKind kind_;
  double number_;
  std::string name_;
  std::vector<ExprPtr> operands_;
};

[[nodiscard]] ExprPtr constant(double value);
[[nodiscard]] ExprPtr variable(std::string name);
[[nodiscard]] ExprPtr add(std::vector<ExprPtr> terms);
[[nodiscard]] ExprPtr subtract(ExprPtr lhs, const ExprPtr& rhs);
[[nodiscard]] ExprPtr multiply(std::vector<ExprPtr> factors);
/// Throws std::invalid_argument if exponent is negative or not finite.
[[nodiscard]] ExprPtr power(ExprPtr base, double exponent);
/// Throws std::invalid_argument if base <= 1 or not finite.
[[nodiscard]] ExprPtr logarithm(ExprPtr argument, double base = 2.0);
[[nodiscard]] ExprPtr summation(std::string index, ExprPtr lower, ExprPtr upper, ExprPtr body);
[[nodiscard]] ExprPtr maximum(std::vector<ExprPtr> operands);

[[nodiscard]] bool isConstant(const Expr& expr, double value) noexcept;

/// Variables that occur free (Summation indices are bound within their body).
[[nodiscard]] std::set<std::string> freeVariables(const Expr& expr);

/// Replaces free occurrences of variable `name` by `replacement` (variables
/// bound by an enclosing Summation with the same name are left untouched).
/// The result is rebuilt with the simplifying factories.
[[nodiscard]] ExprPtr substitute(const ExprPtr& expr, const std::string& name, const ExprPtr& replacement);

/// Human-readable rendering, e.g. "sum_{i=0}^{n - 1} i". Not a parse format.
[[nodiscard]] std::string toString(const Expr& expr);

}  // namespace bigolens::complexity
