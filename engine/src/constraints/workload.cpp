#include "bigolens/constraints/workload.hpp"

#include <algorithm>
#include <cmath>
#include <set>

#include "bigolens/complexity/evaluator.hpp"

namespace bigolens::constraints {

using complexity::Expr;
using complexity::ExprKind;
using complexity::ExprPtr;
using models::BoundStatus;

namespace {

struct Shape {
  bool monotone = false;
  bool nonNegative = false;
};

/// Syntactic monotonicity / sign analysis assuming all variables are >= 0.
Shape shapeOf(const Expr& expr) {
  switch (expr.kind()) {
    case ExprKind::Constant:
      return {true, expr.number() >= 0.0};
    case ExprKind::Variable:
      return {true, true};
    case ExprKind::Add: {
      Shape s{true, true};
      for (const auto& term : expr.operands()) {
        const Shape t = shapeOf(*term);
        // Constants of any sign keep monotonicity; other terms must be monotone.
        s.monotone = s.monotone && t.monotone;
        s.nonNegative = s.nonNegative && t.nonNegative;
      }
      return s;
    }
    case ExprKind::Multiply: {
      Shape s{true, true};
      for (const auto& factor : expr.operands()) {
        const Shape f = shapeOf(*factor);
        s.monotone = s.monotone && f.monotone && f.nonNegative;
        s.nonNegative = s.nonNegative && f.nonNegative;
      }
      return s;
    }
    case ExprKind::Power: {
      const Shape base = shapeOf(*expr.operands().front());
      return {base.monotone && base.nonNegative, base.nonNegative};
    }
    case ExprKind::Logarithm:
      return {shapeOf(*expr.operands().front()).monotone, true};
    case ExprKind::Maximum: {
      Shape s{true, false};
      for (const auto& operand : expr.operands()) {
        const Shape o = shapeOf(*operand);
        s.monotone = s.monotone && o.monotone;
        s.nonNegative = s.nonNegative || o.nonNegative;
      }
      return s;
    }
    case ExprKind::Summation: {
      const Expr& lower = *expr.operands()[0];
      const Shape upper = shapeOf(*expr.operands()[1]);
      const Shape body = shapeOf(*expr.operands()[2]);
      const bool constantLower = complexity::freeVariables(lower).empty();
      return {constantLower && upper.monotone && body.monotone && body.nonNegative, body.nonNegative};
    }
  }
  return {};
}

bool dependsOn(const Expr& expr, const std::string& name) {
  return complexity::freeVariables(expr).count(name) != 0;
}

bool superadditive(const Expr& expr, const std::string& v) {
  if (!dependsOn(expr, v)) {
    return expr.kind() == ExprKind::Constant && expr.number() <= 0.0;
  }
  switch (expr.kind()) {
    case ExprKind::Variable:
      return true;
    case ExprKind::Add:
      return std::all_of(expr.operands().begin(), expr.operands().end(),
                         [&](const ExprPtr& term) { return superadditive(*term, v); });
    case ExprKind::Multiply: {
      bool found = false;
      for (const auto& factor : expr.operands()) {
        const Shape f = shapeOf(*factor);
        if (!f.monotone || !f.nonNegative) {
          return false;
        }
        found = found || superadditive(*factor, v);
      }
      return found;
    }
    case ExprKind::Power: {
      const Expr& base = *expr.operands().front();
      return expr.number() >= 1.0 && shapeOf(base).nonNegative && superadditive(base, v);
    }
    case ExprKind::Maximum: {
      // max(c <= 0, f) with f superadditive and monotone.
      int dependent = 0;
      for (const auto& operand : expr.operands()) {
        if (dependsOn(*operand, v)) {
          ++dependent;
          if (!superadditive(*operand, v) || !shapeOf(*operand).monotone) {
            return false;
          }
        } else if (!(operand->kind() == ExprKind::Constant && operand->number() <= 0.0)) {
          return false;
        }
      }
      return dependent == 1;
    }
    case ExprKind::Summation: {
      // sum_{i=0}^{v + c} g(i), c <= -1, g >= 0 non-decreasing and independent of v.
      const Expr& lower = *expr.operands()[0];
      const Expr& upper = *expr.operands()[1];
      const Expr& body = *expr.operands()[2];
      if (!complexity::isConstant(lower, 0.0) || dependsOn(body, v)) {
        return false;
      }
      if (upper.kind() != ExprKind::Add || upper.operands().size() != 2) {
        return false;
      }
      const Expr& first = *upper.operands()[0];
      const Expr& second = *upper.operands()[1];
      const bool shapeOk = first.kind() == ExprKind::Variable && first.name() == v &&
                           second.kind() == ExprKind::Constant && second.number() <= -1.0;
      const Shape b = shapeOf(body);
      return shapeOk && b.monotone && b.nonNegative;
    }
    case ExprKind::Logarithm:
    case ExprKind::Constant:
      return false;
  }
  return false;
}

/// If expr == t * rest (rest independent of t), returns rest.
ExprPtr stripCountFactor(const Expr& expr, const std::string& count) {
  if (expr.kind() != ExprKind::Multiply) {
    return nullptr;
  }
  std::vector<ExprPtr> rest;
  int occurrences = 0;
  for (const auto& factor : expr.operands()) {
    const bool isCount = factor->kind() == ExprKind::Variable && factor->name() == count;
    const bool isClampedCount = factor->kind() == ExprKind::Maximum && factor->operands().size() == 2 &&
                                complexity::isConstant(*factor->operands()[0], 0.0) &&
                                factor->operands()[1]->kind() == ExprKind::Variable &&
                                factor->operands()[1]->name() == count;
    if (isCount || isClampedCount) {
      ++occurrences;
    } else {
      rest.push_back(factor);
    }
  }
  if (occurrences != 1) {
    return nullptr;
  }
  ExprPtr product = complexity::multiply(std::move(rest));
  return dependsOn(*product, count) ? nullptr : product;
}

std::string formatInt(std::int64_t v) {
  return std::to_string(v);
}

}  // namespace

bool isMonotoneNonDecreasing(const Expr& expr) {
  return shapeOf(expr).monotone;
}

ConstraintSet singleInstanceConstraints(const ConstraintSet& constraints) {
  ConstraintSet single = constraints;
  for (const auto& sum : constraints.sums) {
    auto it = single.variables.find(sum.variable);
    if (it == single.variables.end()) {
      continue;
    }
    std::optional<std::int64_t>& max = it->second.max;
    if (max.has_value()) {
      max = std::min(max.value(), sum.maxTotal);
    }
  }
  single.sums.clear();
  return single;
}

bool isSuperadditiveIn(const Expr& expr, const std::string& variable) {
  const Shape s = shapeOf(expr);
  return s.monotone && s.nonNegative && superadditive(expr, variable);
}

WorkloadResult evaluateWorkload(const Expr& expr, const ConstraintSet& cs) {
  WorkloadResult result;
  const std::set<std::string> free = complexity::freeVariables(expr);

  complexity::Bindings maxima;
  bool negativeAllowed = false;
  for (const auto& name : free) {
    const auto it = cs.variables.find(name);
    if (it == cs.variables.end() || !it->second.max) {
      result.missing.push_back(name);
      continue;
    }
    maxima[name] = static_cast<double>(*it->second.max);
    if (it->second.min && *it->second.min < 0) {
      negativeAllowed = true;
    }
  }
  if (!result.missing.empty()) {
    result.status = BoundStatus::Unknown;
    for (const auto& name : result.missing) {
      result.assumptions.push_back("no upper bound configured for '" + name + "'");
    }
    return result;
  }

  const bool monotone = isMonotoneNonDecreasing(expr) && !negativeAllowed;
  BoundStatus status = monotone ? BoundStatus::Proven : BoundStatus::Estimated;
  if (!monotone) {
    result.assumptions.push_back(
        "expression is not provably non-decreasing; evaluated at variable maxima, which may not be the worst case");
  }
  for (const auto& name : free) {
    const auto& bound = cs.variables.find(name)->second;
    if (!bound.min) {
      result.assumptions.push_back("'" + name + "' assumed non-negative (no lower bound configured)");
    }
  }

  // Sum constraints on variables of the expression.
  std::vector<const SumConstraint*> sums;
  for (const auto& name : free) {
    if (const SumConstraint* sc = findSumConstraint(cs, name)) {
      sums.push_back(sc);
    }
  }

  auto finish = [&](const complexity::EvalResult& eval, std::string method) {
    if (!eval.ok()) {
      result.status = BoundStatus::Unknown;
      result.assumptions.push_back("evaluation failed: " + eval.detail);
      return result;
    }
    result.status = status;
    result.upperBound = eval.value;
    result.method = std::move(method);
    return result;
  };

  for (const auto& product : cs.products) {
    const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                       [&](const std::string& v) { return free.count(v) != 0; });
    if (covered >= 2) {
      result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                   ") not used to tighten the bound; result may be loose");
    }
  }
  if (!cs.linear.empty()) {
    result.assumptions.push_back("linear constraints are not yet used to tighten workload bounds");
  }

  if (sums.empty()) {
    return finish(complexity::evaluate(expr, maxima), "pointwise-maxima");
  }

  const std::string& countVar = sums.front()->countVariable;
  const bool sameCount =
      std::all_of(sums.begin(), sums.end(), [&](const SumConstraint* sc) { return sc->countVariable == countVar; });
  if (!sameCount) {
    result.status = BoundStatus::Unknown;
    result.assumptions.push_back("sum constraints over different test-case counts are not supported");
    return result;
  }
  const double maxCases = static_cast<double>(*cs.variables.find(countVar)->second.max);

  // Per-case value of every sum-constrained variable is at most min(max, total).
  complexity::Bindings capped = maxima;
  for (const SumConstraint* sc : sums) {
    capped[sc->variable] = std::min(capped[sc->variable], static_cast<double>(sc->maxTotal));
  }

  ExprPtr perCase;
  if (free.count(countVar) != 0) {
    perCase = stripCountFactor(expr, countVar);
    if (!perCase) {
      // The count variable appears in a shape we cannot separate; per-case caps still give a sound bound.
      capped[countVar] = maxCases;
      result.assumptions.push_back("sum constraint over '" + countVar +
                                   "' not fully exploited; each case evaluated at its individual maximum");
      return finish(complexity::evaluate(expr, capped), "pointwise-maxima-capped");
    }
  } else {
    perCase = std::make_shared<const Expr>(expr);
    result.assumptions.push_back("cost treated as per test case because '" + sums.front()->variable +
                                 "' is constrained across test cases ('" + countVar + "')");
  }

  if (sums.size() == 1 && monotone && isSuperadditiveIn(*perCase, sums.front()->variable)) {
    const SumConstraint& sc = *sums.front();
    const double total = static_cast<double>(sc.maxTotal);
    const double perCaseMax = capped[sc.variable];
    const double fullCases = perCaseMax > 0.0 ? std::min(maxCases, std::floor(total / perCaseMax)) : 0.0;
    const double remainder = total - fullCases * perCaseMax;

    complexity::EvalResult full = complexity::evaluate(*perCase, capped);
    if (!full.ok()) {
      return finish(full, "");
    }
    double value = fullCases * full.value;
    if (fullCases < maxCases && remainder > 0.0) {
      complexity::Bindings rest = capped;
      rest[sc.variable] = remainder;
      complexity::EvalResult partial = complexity::evaluate(*perCase, rest);
      if (!partial.ok()) {
        return finish(partial, "");
      }
      value += partial.value;
    }
    result.assumptions.push_back("worst case packs " + formatInt(static_cast<std::int64_t>(fullCases)) +
                                 " test case(s) of maximum size (superadditive per-case cost)");
    return finish(complexity::EvalResult{complexity::EvalStatus::Ok, value, {}}, "sum-constraint-packing");
  }

  // Sound fallback for monotone costs: (number of cases) x (per-case maximum).
  double cases = maxCases;
  for (const SumConstraint* sc : sums) {
    const auto& bound = cs.variables.find(sc->variable)->second;
    if (bound.min && *bound.min >= 1) {
      cases = std::min(cases, std::floor(static_cast<double>(sc->maxTotal) / static_cast<double>(*bound.min)));
    }
  }
  complexity::EvalResult per = complexity::evaluate(*perCase, capped);
  if (!per.ok()) {
    return finish(per, "");
  }
  result.assumptions.push_back("per-case cost not recognised as superadditive; bound is cases x per-case maximum");
  return finish(complexity::EvalResult{complexity::EvalStatus::Ok, cases * per.value, {}}, "sum-constraint-cases");
}

}  // namespace bigolens::constraints
