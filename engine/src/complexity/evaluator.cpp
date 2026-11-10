#include "bigolens/complexity/evaluator.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <optional>
#include <utility>

namespace bigolens::complexity {
namespace {

/// Coefficients c0 + c1 * index of an expression affine in `index`.
struct Affine {
  ExprPtr c0;
  ExprPtr c1;
};

bool dependsOn(const Expr& expr, const std::string& name) {
  return freeVariables(expr).count(name) != 0;
}

/// Structural (not numeric) affinity check; returns coefficients as expressions.
std::optional<Affine> asAffine(const ExprPtr& expr, const std::string& index) {
  if (!dependsOn(*expr, index)) {
    return Affine{expr, constant(0.0)};
  }
  switch (expr->kind()) {
    case ExprKind::Variable:
      return Affine{constant(0.0), constant(1.0)};
    case ExprKind::Add: {
      std::vector<ExprPtr> c0s;
      std::vector<ExprPtr> c1s;
      for (const auto& term : expr->operands()) {
        auto affine = asAffine(term, index);
        if (!affine) {
          return std::nullopt;
        }
        c0s.push_back(affine->c0);
        c1s.push_back(affine->c1);
      }
      return Affine{add(std::move(c0s)), add(std::move(c1s))};
    }
    case ExprKind::Multiply: {
      std::vector<ExprPtr> others;
      ExprPtr dependent;
      for (const auto& factor : expr->operands()) {
        if (dependsOn(*factor, index)) {
          if (dependent) {
            return std::nullopt;  // product of two index-dependent factors
          }
          dependent = factor;
        } else {
          others.push_back(factor);
        }
      }
      auto affine = asAffine(dependent, index);
      if (!affine) {
        return std::nullopt;
      }
      std::vector<ExprPtr> f0 = others;
      f0.push_back(affine->c0);
      std::vector<ExprPtr> f1 = others;
      f1.push_back(affine->c1);
      return Affine{multiply(std::move(f0)), multiply(std::move(f1))};
    }
    default:
      return std::nullopt;
  }
}

class Evaluator {
 public:
  Evaluator(Bindings bindings, std::uint64_t budget) : bindings_(std::move(bindings)), budget_(budget) {}

  EvalResult run(const Expr& expr) {
    if (budget_ == 0) {
      return fail(EvalStatus::BudgetExceeded, "evaluation budget exceeded");
    }
    --budget_;
    EvalResult result = dispatch(expr);
    if (result.ok() && !std::isfinite(result.value)) {
      return fail(EvalStatus::Overflow, "value exceeds the representable range");
    }
    return result;
  }

 private:
  static EvalResult fail(EvalStatus status, std::string detail) { return EvalResult{status, 0.0, std::move(detail)}; }
  static EvalResult value(double v) { return EvalResult{EvalStatus::Ok, v, {}}; }

  EvalResult dispatch(const Expr& expr) {
    switch (expr.kind()) {
      case ExprKind::Constant:
        return value(expr.number());
      case ExprKind::Variable: {
        const auto it = bindings_.find(expr.name());
        if (it == bindings_.end()) {
          return fail(EvalStatus::UnboundVariable, "no value for variable '" + expr.name() + "'");
        }
        return value(it->second);
      }
      case ExprKind::Add:
      case ExprKind::Multiply:
      case ExprKind::Maximum: {
        double acc = expr.kind() == ExprKind::Multiply ? 1.0 : 0.0;
        bool first = true;
        for (const auto& operand : expr.operands()) {
          EvalResult r = run(*operand);
          if (!r.ok()) {
            return r;
          }
          if (expr.kind() == ExprKind::Add) {
            acc += r.value;
          } else if (expr.kind() == ExprKind::Multiply) {
            acc *= r.value;
          } else {
            acc = first ? r.value : std::max(acc, r.value);
          }
          first = false;
          if (!std::isfinite(acc)) {
            return fail(EvalStatus::Overflow, "value exceeds the representable range");
          }
        }
        return value(acc);
      }
      case ExprKind::Power: {
        EvalResult base = run(*expr.operands().front());
        if (!base.ok()) {
          return base;
        }
        if (base.value < 0.0 && expr.number() != std::floor(expr.number())) {
          return fail(EvalStatus::DomainError, "fractional power of a negative value");
        }
        return value(std::pow(base.value, expr.number()));
      }
      case ExprKind::Logarithm: {
        EvalResult arg = run(*expr.operands().front());
        if (!arg.ok()) {
          return arg;
        }
        return value(std::log(std::max(arg.value, 1.0)) / std::log(expr.number()));
      }
      case ExprKind::Summation:
        return summation(expr);
    }
    return fail(EvalStatus::DomainError, "unknown expression kind");
  }

  EvalResult summation(const Expr& expr) {
    EvalResult lower = run(*expr.operands()[0]);
    if (!lower.ok()) {
      return lower;
    }
    EvalResult upper = run(*expr.operands()[1]);
    if (!upper.ok()) {
      return upper;
    }
    const double lo = std::ceil(lower.value);
    const double hi = std::floor(upper.value);
    if (hi < lo) {
      return value(0.0);
    }
    const double count = hi - lo + 1.0;
    const std::string& index = expr.name();
    const ExprPtr& body = expr.operands()[2];

    // Closed form for bodies that are affine in the index:
    // sum_{i=lo}^{hi} (c0 + c1 i) = count * c0 + c1 * (lo + hi) * count / 2.
    if (auto affine = asAffine(body, index)) {
      EvalResult c0 = withIndexHidden(index, *affine->c0);
      if (!c0.ok()) {
        return c0;
      }
      EvalResult c1 = withIndexHidden(index, *affine->c1);
      if (!c1.ok()) {
        return c1;
      }
      return value(count * c0.value + c1.value * (lo + hi) * count / 2.0);
    }

    // Term-by-term evaluation, bounded by the budget.
    if (count > static_cast<double>(budget_)) {
      return fail(EvalStatus::BudgetExceeded, "summation over '" + index + "' is too large to evaluate term by term");
    }
    const auto previous = bindings_.find(index);
    const std::optional<double> saved =
        previous == bindings_.end() ? std::nullopt : std::optional<double>(previous->second);
    double acc = 0.0;
    EvalResult outcome = value(0.0);
    const auto terms = static_cast<std::uint64_t>(count);  // count <= budget, so exact.
    for (std::uint64_t k = 0; k < terms; ++k) {
      bindings_[index] = lo + static_cast<double>(k);
      EvalResult term = run(*body);
      if (!term.ok()) {
        outcome = term;
        break;
      }
      acc += term.value;
      if (!std::isfinite(acc)) {
        outcome = fail(EvalStatus::Overflow, "value exceeds the representable range");
        break;
      }
    }
    if (saved) {
      bindings_[index] = *saved;
    } else {
      bindings_.erase(index);
    }
    return outcome.ok() ? value(acc) : outcome;
  }

  /// Coefficients do not depend on the index; evaluate them with any outer
  /// binding of the same name hidden so shadowing is respected.
  EvalResult withIndexHidden(const std::string& index, const Expr& expr) {
    const auto it = bindings_.find(index);
    if (it == bindings_.end()) {
      return run(expr);
    }
    const double saved = it->second;
    bindings_.erase(it);
    EvalResult r = run(expr);
    bindings_[index] = saved;
    return r;
  }

  Bindings bindings_;
  std::uint64_t budget_;
};

}  // namespace

EvalResult evaluate(const Expr& expr, const Bindings& bindings, const EvalOptions& options) {
  Evaluator evaluator(bindings, options.budget);
  return evaluator.run(expr);
}

}  // namespace bigolens::complexity
