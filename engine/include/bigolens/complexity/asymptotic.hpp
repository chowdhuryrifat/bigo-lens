#pragma once

#include <map>
#include <optional>
#include <string>
#include <vector>

#include "bigolens/complexity/expression.hpp"

namespace bigolens::complexity {

/// v^power * (log v)^logPower for one variable.
struct GrowthFactor {
  double power = 0.0;
  double logPower = 0.0;
  friend bool operator==(const GrowthFactor&, const GrowthFactor&) = default;
};

/// Product of growth factors over distinct variables; empty means constant.
using Monomial = std::map<std::string, GrowthFactor>;

/// An asymptotic upper bound expressed as a sum of mutually non-dominated
/// monomials, e.g. {n^2, n*m} for O(n^2 + n m).
struct AsymptoticBound {
  std::vector<Monomial> terms;
};

/// Derives an O(...) upper bound assuming all variables grow without bound and
/// are >= 1. Terms with negative coefficients are dropped, which keeps the
/// result an upper bound. Returns std::nullopt for unsupported shapes
/// (for example a logarithm of a logarithm).
[[nodiscard]] std::optional<AsymptoticBound> asymptoticUpperBound(const Expr& expr);

/// Renders a bound as "O(n^2 log n)".
[[nodiscard]] std::string toString(const AsymptoticBound& bound);

}  // namespace bigolens::complexity
