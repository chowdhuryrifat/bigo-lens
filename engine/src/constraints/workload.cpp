#include "bigolens/constraints/workload.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <map>
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

struct ConstraintContext {
  const std::vector<ProductConstraint>& products;
  const std::vector<LinearConstraint>& linear;
  const complexity::Bindings& minima;
  std::vector<bool> productApplied;
  std::vector<bool> productTightened;
  std::vector<bool> linearApplied;
  std::vector<bool> linearTightened;
  bool remainingFactorsAtMaxima = false;

  ConstraintContext(const std::vector<ProductConstraint>& prods, const std::vector<LinearConstraint>& lins,
                    const complexity::Bindings& mins)
      : products(prods),
        linear(lins),
        minima(mins),
        productApplied(prods.size(), false),
        productTightened(prods.size(), false),
        linearApplied(lins.size(), false),
        linearTightened(lins.size(), false) {}
};

struct LinearTerm {
  std::string name;
  double coeff = 1.0;
};

std::optional<LinearTerm> asLinearTerm(const Expr& expr) {
  if (expr.kind() == ExprKind::Variable) {
    return LinearTerm{expr.name(), 1.0};
  }
  if (expr.kind() == ExprKind::Multiply && expr.operands().size() == 2) {
    const auto& op0 = expr.operands()[0];
    const auto& op1 = expr.operands()[1];
    if (op0->kind() == ExprKind::Constant && op0->number() > 0.0 && op1->kind() == ExprKind::Variable) {
      return LinearTerm{op1->name(), op0->number()};
    }
    if (op1->kind() == ExprKind::Constant && op1->number() > 0.0 && op0->kind() == ExprKind::Variable) {
      return LinearTerm{op0->name(), op1->number()};
    }
  }
  return std::nullopt;
}

void tightenVariableBoundsWithLinear(complexity::Bindings& maxima, const complexity::Bindings& minima,
                                     const std::vector<LinearConstraint>& linear, std::vector<bool>& linearApplied,
                                     std::vector<bool>& linearTightened) {
  bool changed = true;
  for (int round = 0; round < 5 && changed; ++round) {
    changed = false;
    for (size_t i = 0; i < linear.size(); ++i) {
      const auto& lc = linear[i];
      bool allNonNeg = true;
      for (const auto& [name, coeff] : lc.coefficients) {
        if (coeff < 0) {
          allNonNeg = false;
          break;
        }
      }
      if (!allNonNeg) {
        continue;
      }

      for (const auto& [v, coeff] : lc.coefficients) {
        if (coeff <= 0) {
          continue;
        }
        auto it = maxima.find(v);
        if (it == maxima.end()) {
          continue;
        }

        double minOther = 0.0;
        bool otherOk = true;
        for (const auto& [u, ucoeff] : lc.coefficients) {
          if (u == v || ucoeff <= 0) {
            continue;
          }
          auto mit = minima.find(u);
          if (mit == minima.end() || mit->second < 0.0) {
            otherOk = false;
            break;
          }
          minOther += static_cast<double>(ucoeff) * mit->second;
        }
        if (!otherOk) {
          continue;
        }

        double remaining = static_cast<double>(lc.bound) - minOther;
        if (remaining >= 0.0) {
          double cap = std::floor(remaining / static_cast<double>(coeff));
          if (cap < it->second) {
            it->second = cap;
            linearApplied[i] = true;
            linearTightened[i] = true;
            changed = true;
          }
        }
      }
    }
  }
}

struct VarPower {
  std::string name;
  double exponent = 0.0;
};

std::optional<VarPower> asVarPower(const Expr& expr) {
  if (expr.kind() == ExprKind::Variable) {
    return VarPower{expr.name(), 1.0};
  }
  if (expr.kind() == ExprKind::Power && expr.operands().size() == 1 &&
      expr.operands().front()->kind() == ExprKind::Variable && expr.number() > 0.0) {
    return VarPower{expr.operands().front()->name(), expr.number()};
  }
  return std::nullopt;
}

complexity::EvalResult evalMonotoneWithConstraints(const Expr& expr, const complexity::Bindings& bindings,
                                                   ConstraintContext& ctx);

/// Evaluates an Add expression under monotone variable bounds and applies linear constraints
/// to tighten the sum bound when terms are bounded by linear combinations (e.g. n + m <= 2e5).
complexity::EvalResult evalAddWithConstraints(const Expr& expr, const complexity::Bindings& bindings,
                                              ConstraintContext& ctx) {
  double acc = 0.0;
  for (const auto& term : expr.operands()) {
    auto r = evalMonotoneWithConstraints(*term, bindings, ctx);
    if (!r.ok()) {
      return r;
    }
    acc += r.value;
    if (!std::isfinite(acc)) {
      return complexity::EvalResult{complexity::EvalStatus::Overflow, 0.0, "value exceeds the representable range"};
    }
  }

  std::map<std::string, double> linTerms;
  double nonLinSum = 0.0;
  for (const auto& term : expr.operands()) {
    if (auto lt = asLinearTerm(*term)) {
      linTerms[lt->name] += lt->coeff;
    } else {
      auto r = evalMonotoneWithConstraints(*term, bindings, ctx);
      if (r.ok()) {
        nonLinSum += r.value;
      }
    }
  }
  if (linTerms.size() >= 2) {
    for (size_t i = 0; i < ctx.linear.size(); ++i) {
      const auto& lc = ctx.linear[i];
      bool allCoeffsPos = true;
      for (const auto& [name, coeff] : lc.coefficients) {
        if (coeff < 0) {
          allCoeffsPos = false;
          break;
        }
      }
      if (!allCoeffsPos) {
        continue;
      }

      bool coversAll = true;
      double maxRatio = 0.0;
      for (const auto& [name, alpha] : linTerms) {
        auto cit = lc.coefficients.find(name);
        if (cit == lc.coefficients.end() || cit->second <= 0) {
          coversAll = false;
          break;
        }
        double ratio = alpha / static_cast<double>(cit->second);
        if (ratio > maxRatio) {
          maxRatio = ratio;
        }
      }
      if (!coversAll) {
        continue;
      }

      double minOther = 0.0;
      for (const auto& [u, ucoeff] : lc.coefficients) {
        if (linTerms.count(u) == 0 && ucoeff > 0) {
          auto mit = ctx.minima.find(u);
          if (mit != ctx.minima.end()) {
            minOther += static_cast<double>(ucoeff) * mit->second;
          }
        }
      }
      double beff = std::max(0.0, static_cast<double>(lc.bound) - minOther);
      double linSumBound = maxRatio * beff + nonLinSum;
      if (linSumBound < acc) {
        acc = linSumBound;
        ctx.linearApplied[i] = true;
        ctx.linearTightened[i] = true;
      }
    }
  }
  return complexity::EvalResult{complexity::EvalStatus::Ok, acc, {}};
}

/// Evaluates a Multiply expression under monotone variable bounds and applies product
/// and linear constraints to find tight bounds for composite terms.
///
/// Flattens nested multiplication terms, groups variable powers, and iteratively applies
/// the constraint (product or linear combination) that yields the greatest reduction ratio
/// over the naive pointwise product of maxima.
///
/// @param expr The multiplication expression AST node to evaluate.
/// @param bindings Maximum known upper bounds for free variables.
/// @param ctx Active constraint context tracking applied constraints and minimum bounds.
/// @return EvalResult containing the tightened upper bound or evaluation error.
complexity::EvalResult evalMultiplyWithConstraints(const Expr& expr, const complexity::Bindings& bindings,
                                                   ConstraintContext& ctx) {
  // Step 1: Recursively flatten all associative nested Multiply operations into a single factor list.
  std::vector<ExprPtr> flat;
  std::function<void(const ExprPtr&)> flatten = [&](const ExprPtr& factor) {
    if (factor->kind() == ExprKind::Multiply) {
      for (const auto& child : factor->operands()) {
        flatten(child);
      }
    } else {
      flat.push_back(factor);
    }
  };
  for (const auto& op : expr.operands()) {
    flatten(op);
  }

  // Step 2: Separate factors into variable powers (e.g. n^2 * m) and other non-variable factors.
  std::map<std::string, double> varExponents;
  std::vector<ExprPtr> otherFactors;
  for (const auto& f : flat) {
    if (auto vp = asVarPower(*f)) {
      varExponents[vp->name] += vp->exponent;
    } else {
      otherFactors.push_back(f);
    }
  }

  // Running accumulator for bounded factors.
  double acc = 1.0;

  // Step 3: Iteratively select and apply the constraint that produces the greatest reduction ratio.
  while (true) {
    enum class CandidateType { None, Product, Linear };
    CandidateType bestType = CandidateType::None;
    size_t bestIdx = 0;
    double bestReduction = 1.0;

    double bestProductAmin = 0.0;
    double bestProductPeff = 0.0;
    double bestProductMprod = 0.0;

    double bestLinearBound = 0.0;
    std::vector<std::string> bestLinearVars;

    // Step 3a: Evaluate candidate Product Constraints (e.g. n * m <= 1e5).
    for (size_t i = 0; i < ctx.products.size(); ++i) {
      const auto& pc = ctx.products[i];
      bool allPresent = true;
      double amin = 0.0;
      double mprod = 1.0;
      for (size_t vi = 0; vi < pc.variables.size(); ++vi) {
        const auto& v = pc.variables[vi];
        auto it = varExponents.find(v);
        if (it == varExponents.end() || it->second < 1.0) {
          allPresent = false;
          break;
        }
        if (vi == 0 || it->second < amin) {
          amin = it->second;
        }
        auto bit = bindings.find(v);
        if (bit == bindings.end()) {
          allPresent = false;
          break;
        }
        mprod *= bit->second;
      }
      if (!allPresent || amin < 1.0) {
        continue;
      }
      // Compute the effective product bound and the resulting reduction ratio.
      double peff = std::min(static_cast<double>(pc.maxProduct), mprod);
      double reduction = (mprod > 0.0) ? std::pow(peff / mprod, amin) : 1.0;
      if (reduction < bestReduction || (bestType == CandidateType::None && peff <= mprod)) {
        bestReduction = reduction;
        bestType = CandidateType::Product;
        bestIdx = i;
        bestProductAmin = amin;
        bestProductPeff = peff;
        bestProductMprod = mprod;
      }
    }

    // Step 3b: Evaluate candidate Linear Constraints (e.g. n + m <= 2e5).
    for (size_t j = 0; j < ctx.linear.size(); ++j) {
      const auto& lc = ctx.linear[j];
      bool allNonNeg = true;
      for (const auto& [name, coeff] : lc.coefficients) {
        if (coeff < 0) {
          allNonNeg = false;
          break;
        }
      }
      if (!allNonNeg) {
        continue;
      }

      // Collect variables that appear in this linear constraint and have remaining positive exponent.
      std::vector<std::string> matchedVars;
      for (const auto& [v, coeff] : lc.coefficients) {
        if (coeff > 0) {
          auto it = varExponents.find(v);
          if (it != varExponents.end() && it->second >= 1.0) {
            matchedVars.push_back(v);
          }
        }
      }

      // Optimization case: 2-variable linear constraint (e.g. c_x * x + c_y * y <= B).
      if (matchedVars.size() == 2) {
        const std::string& x = matchedVars[0];
        const std::string& y = matchedVars[1];
        double cx = static_cast<double>(lc.coefficients.at(x));
        double cy = static_cast<double>(lc.coefficients.at(y));
        double ax = varExponents.at(x);
        double ay = varExponents.at(y);
        double mx = ctx.minima.count(x) ? ctx.minima.at(x) : 0.0;
        double my = ctx.minima.count(y) ? ctx.minima.at(y) : 0.0;
        double Mx = bindings.at(x);
        double My = bindings.at(y);

        // Subtract minimum contributions from other variables appearing in the constraint.
        double minOther = 0.0;
        for (const auto& [u, ucoeff] : lc.coefficients) {
          if (u != x && u != y && ucoeff > 0) {
            double mu = ctx.minima.count(u) ? ctx.minima.at(u) : 0.0;
            minOther += static_cast<double>(ucoeff) * mu;
          }
        }
        double beff = static_cast<double>(lc.bound) - minOther;
        if (beff > 0.0) {
          // Use Lagrange multiplier / AM-GM unconstrained optimal point: x* = (a_x / (a_x + a_y)) * (B / c_x).
          double xstar = (ax / (ax + ay)) * (beff / cx);
          // Clamp optimal point to the feasible domain defined by variable bounds.
          double xlo = std::max(mx, (beff - cy * My) / cx);
          double xhi = std::min(Mx, (beff - cy * my) / cx);
          if (xlo <= xhi) {
            double xopt = std::clamp(xstar, xlo, xhi);
            double yopt = std::max(0.0, (beff - cx * xopt) / cy);
            double linBound = std::pow(xopt, ax) * std::pow(yopt, ay);
            double mprod = std::pow(Mx, ax) * std::pow(My, ay);
            double reduction = (mprod > 0.0) ? (linBound / mprod) : 1.0;
            if (reduction < bestReduction || (bestType == CandidateType::None && linBound <= mprod)) {
              bestReduction = reduction;
              bestType = CandidateType::Linear;
              bestIdx = j;
              bestLinearBound = linBound;
              bestLinearVars = {x, y};
            }
          }
        }
      } else if (matchedVars.size() > 2) {
        // Multi-variable linear constraint case (k > 2 variables).
        double sumA = 0.0;
        for (const auto& v : matchedVars) {
          sumA += varExponents.at(v);
        }
        // Account for minimum contributions of non-matched variables.
        double minOther = 0.0;
        for (const auto& [u, ucoeff] : lc.coefficients) {
          if (ucoeff > 0 && std::find(matchedVars.begin(), matchedVars.end(), u) == matchedVars.end()) {
            double mu = ctx.minima.count(u) ? ctx.minima.at(u) : 0.0;
            minOther += static_cast<double>(ucoeff) * mu;
          }
        }
        double beff = static_cast<double>(lc.bound) - minOther;
        if (beff > 0.0 && sumA > 0.0) {
          double linBound = 1.0;
          double mprod = 1.0;
          for (const auto& v : matchedVars) {
            double cv = static_cast<double>(lc.coefficients.at(v));
            double av = varExponents.at(v);
            double mv = ctx.minima.count(v) ? ctx.minima.at(v) : 0.0;
            double Mv = bindings.at(v);
            // Proportional allocation of bound across all matched variables.
            double xvstar = (av / sumA) * (beff / cv);
            double xvopt = std::clamp(xvstar, mv, Mv);
            linBound *= std::pow(xvopt, av);
            mprod *= std::pow(Mv, av);
          }
          double reduction = (mprod > 0.0) ? (linBound / mprod) : 1.0;
          if (reduction < bestReduction || (bestType == CandidateType::None && linBound <= mprod)) {
            bestReduction = reduction;
            bestType = CandidateType::Linear;
            bestIdx = j;
            bestLinearBound = linBound;
            bestLinearVars = matchedVars;
          }
        }
      }
    }

    // Step 3c: Stop when no candidate constraint yields further bound tightening.
    if (bestType == CandidateType::None) {
      break;
    }

    // Step 3d: Apply selected constraint reduction and deduct exponents from consumed variables.
    if (bestType == CandidateType::Product) {
      const auto& bestPc = ctx.products[bestIdx];
      ctx.productApplied[bestIdx] = true;
      if (bestProductPeff < bestProductMprod) {
        ctx.productTightened[bestIdx] = true;
      }
      acc *= std::pow(bestProductPeff, bestProductAmin);
      for (const auto& v : bestPc.variables) {
        if (varExponents[v] > bestProductAmin) {
          ctx.remainingFactorsAtMaxima = true;
        }
        varExponents[v] -= bestProductAmin;
      }
    } else if (bestType == CandidateType::Linear) {
      ctx.linearApplied[bestIdx] = true;
      ctx.linearTightened[bestIdx] = true;
      acc *= bestLinearBound;
      for (const auto& v : bestLinearVars) {
        varExponents[v] = 0.0;
      }
    }
  }

  // Step 4: Multiply remaining unconstrained variable powers evaluated at their individual maxima.
  for (const auto& [name, exp] : varExponents) {
    if (exp > 0.0) {
      auto it = bindings.find(name);
      if (it == bindings.end()) {
        return complexity::EvalResult{complexity::EvalStatus::UnboundVariable, 0.0, "unbound variable '" + name + "'"};
      }
      acc *= std::pow(it->second, exp);
    }
  }

  // Step 5: Recursively evaluate all non-variable factors and multiply into accumulator.
  for (const auto& of : otherFactors) {
    auto r = evalMonotoneWithConstraints(*of, bindings, ctx);
    if (!r.ok()) {
      return r;
    }
    acc *= r.value;
  }

  // Step 6: Verify result finiteness to prevent numerical overflow.
  if (!std::isfinite(acc)) {
    return complexity::EvalResult{complexity::EvalStatus::Overflow, 0.0, "value exceeds the representable range"};
  }
  return complexity::EvalResult{complexity::EvalStatus::Ok, acc, {}};
}

/// Evaluates a monotone expression subject to linear and product constraints,
/// maximizing monotone terms under variable bounds and constraint relationships.
complexity::EvalResult evalMonotoneWithConstraints(const Expr& expr, const complexity::Bindings& bindings,
                                                   ConstraintContext& ctx) {
  switch (expr.kind()) {
    case ExprKind::Constant:
      return complexity::EvalResult{complexity::EvalStatus::Ok, expr.number(), {}};
    case ExprKind::Variable: {
      auto it = bindings.find(expr.name());
      if (it == bindings.end()) {
        return complexity::EvalResult{complexity::EvalStatus::UnboundVariable, 0.0,
                                      "unbound variable '" + expr.name() + "'"};
      }
      return complexity::EvalResult{complexity::EvalStatus::Ok, it->second, {}};
    }
    case ExprKind::Add:
      return evalAddWithConstraints(expr, bindings, ctx);
    case ExprKind::Maximum: {
      double acc = 0.0;
      bool first = true;
      for (const auto& op : expr.operands()) {
        auto r = evalMonotoneWithConstraints(*op, bindings, ctx);
        if (!r.ok()) {
          return r;
        }
        if (first || r.value > acc) {
          acc = r.value;
          first = false;
        }
      }
      return complexity::EvalResult{complexity::EvalStatus::Ok, acc, {}};
    }
    case ExprKind::Power: {
      auto r = evalMonotoneWithConstraints(*expr.operands().front(), bindings, ctx);
      if (!r.ok()) {
        return r;
      }
      double v = std::pow(r.value, expr.number());
      if (!std::isfinite(v)) {
        return complexity::EvalResult{complexity::EvalStatus::Overflow, 0.0, "value exceeds the representable range"};
      }
      return complexity::EvalResult{complexity::EvalStatus::Ok, v, {}};
    }
    case ExprKind::Logarithm: {
      auto r = evalMonotoneWithConstraints(*expr.operands().front(), bindings, ctx);
      if (!r.ok()) {
        return r;
      }
      double arg = std::max(r.value, 1.0);
      double v = std::log2(arg) / std::log2(expr.number());
      if (!std::isfinite(v)) {
        return complexity::EvalResult{complexity::EvalStatus::Overflow, 0.0, "value exceeds the representable range"};
      }
      return complexity::EvalResult{complexity::EvalStatus::Ok, v, {}};
    }
    case ExprKind::Summation: {
      return complexity::evaluate(expr, bindings);
    }
    case ExprKind::Multiply:
      return evalMultiplyWithConstraints(expr, bindings, ctx);
  }
  return complexity::EvalResult{complexity::EvalStatus::DomainError, 0.0, "unknown expression kind"};
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

/// Evaluates workload when no sum constraints are active, applying linear and product
/// constraints if the expression is monotone, or falling back to pointwise evaluation.
WorkloadResult evaluateWorkloadWithoutSums(const Expr& expr, const ConstraintSet& cs, const std::set<std::string>& free,
                                           complexity::Bindings maxima, const complexity::Bindings& minima,
                                           bool monotone, BoundStatus status, ConstraintContext& ctx,
                                           WorkloadResult result) {
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

  if (monotone) {
    tightenVariableBoundsWithLinear(maxima, minima, cs.linear, ctx.linearApplied, ctx.linearTightened);
    auto eval = evalMonotoneWithConstraints(expr, maxima, ctx);

    bool anyProductTightened = false;
    for (size_t i = 0; i < cs.products.size(); ++i) {
      if (ctx.productTightened[i]) {
        anyProductTightened = true;
      }
    }
    for (size_t i = 0; i < cs.products.size(); ++i) {
      const auto& product = cs.products[i];
      const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                         [&](const std::string& v) { return free.count(v) != 0; });
      if (covered >= 2 && !ctx.productApplied[i]) {
        result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                     ") not used to tighten the bound; result may be loose");
      }
    }

    bool anyLinearTightened = false;
    for (size_t i = 0; i < cs.linear.size(); ++i) {
      if (ctx.linearTightened[i]) {
        anyLinearTightened = true;
      }
    }
    for (size_t i = 0; i < cs.linear.size(); ++i) {
      const auto& lc = cs.linear[i];
      bool hasNeg = false;
      for (const auto& [name, coeff] : lc.coefficients) {
        if (coeff < 0) {
          hasNeg = true;
          break;
        }
      }
      if (hasNeg) {
        result.assumptions.push_back("linear constraint with negative coefficient (bound " + formatInt(lc.bound) +
                                     ") not used to tighten the bound; result may be loose");
        continue;
      }
      const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                         [&](const auto& pair) { return free.count(pair.first) != 0; });
      if (covered >= 2 && !ctx.linearApplied[i]) {
        result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                     ") not used to tighten the bound; result may be loose");
      }
    }

    std::string method = "pointwise-maxima";
    if (anyProductTightened) {
      method = "product-constraint-bound";
    } else if (anyLinearTightened) {
      method = "linear-constraint-bound";
    }
    if (anyProductTightened && ctx.remainingFactorsAtMaxima) {
      result.assumptions.push_back(
          "workload bounded by product constraint with remaining factors evaluated at individual maxima");
    }
    return finish(eval, std::move(method));
  }

  // Handle non-monotone expressions where bounds cannot be tightened with constraints.
  for (const auto& product : cs.products) {
    const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                       [&](const std::string& v) { return free.count(v) != 0; });
    if (covered >= 2) {
      result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                   ") not used to tighten the bound; result may be loose");
    }
  }
  for (const auto& lc : cs.linear) {
    const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                       [&](const auto& pair) { return free.count(pair.first) != 0; });
    if (covered >= 2) {
      result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                   ") not used to tighten the bound on non-monotone expression; result may be loose");
    }
  }
  return finish(complexity::evaluate(expr, maxima), "pointwise-maxima");
}

/// Evaluates the worst-case workload of an expression subject to sum constraints over test cases.
///
/// Supports three primary evaluation strategies:
/// 1. Single-sum superadditive packing: when per-case cost is superadditive (e.g. f(n) = n log n),
///    worst-case total cost occurs when packing as many maximal cases as the total sum allows.
/// 2. Multi-sum separable terms packing: when per-case cost decomposes into separable terms where each
///    term depends on at most one sum-constrained variable, packs each term independently.
/// 3. Monotone fallback: soundly bounds workload by (number of test cases) * (per-case maximum).
///
/// @param expr Asymptotic cost expression to evaluate.
/// @param cs Active user-configured constraint set.
/// @param free Free variables occurring in the cost expression.
/// @param sums Vector of active SumConstraint definitions covering free variables.
/// @param maxima Maximum upper bounds for variables.
/// @param monotone Whether the cost expression is provably monotone non-decreasing.
/// @param status Bound status (Proven or Estimated).
/// @param ctx Active constraint context for linear and product bounds.
/// @param result Base WorkloadResult to populate with bound and assumptions.
/// @return Populated WorkloadResult with upper bound and derived assumptions.
WorkloadResult evaluateWorkloadWithSums(const Expr& expr, const ConstraintSet& cs, const std::set<std::string>& free,
                                        const std::vector<const SumConstraint*>& sums, complexity::Bindings maxima,
                                        bool monotone, BoundStatus status, ConstraintContext& ctx,
                                        WorkloadResult result) {
  // Helper to record final status, workload bound, and estimation method.
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

  // Step 1: Verify all active sum constraints share the identical test-case count variable.
  const std::string& countVar = sums.front()->countVariable;
  const bool sameCount =
      std::all_of(sums.begin(), sums.end(), [&](const SumConstraint* sc) { return sc->countVariable == countVar; });
  if (!sameCount) {
    result.status = BoundStatus::Unknown;
    result.assumptions.push_back("sum constraints over different test-case counts are not supported");
    return result;
  }
  const double maxCases = static_cast<double>(*cs.variables.find(countVar)->second.max);

  // Step 2: Per-case value of every sum-constrained variable is at most min(max, total).
  complexity::Bindings capped = std::move(maxima);
  for (const SumConstraint* sc : sums) {
    capped[sc->variable] = std::min(capped[sc->variable], static_cast<double>(sc->maxTotal));
  }

  // Step 3: Strip the test-case count variable factor from the cost expression if present.
  ExprPtr perCase;
  if (free.count(countVar) != 0) {
    perCase = stripCountFactor(expr, countVar);
    if (!perCase) {
      // The count variable appears in a shape we cannot separate; per-case caps still give a sound bound.
      capped[countVar] = maxCases;
      result.assumptions.push_back("sum constraint over '" + countVar +
                                   "' not fully exploited; each case evaluated at its individual maximum");
      for (size_t i = 0; i < cs.products.size(); ++i) {
        const auto& product = cs.products[i];
        const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                           [&](const std::string& v) { return free.count(v) != 0; });
        if (covered >= 2) {
          result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                       ") not used to tighten the bound; result may be loose");
        }
      }
      for (size_t i = 0; i < cs.linear.size(); ++i) {
        const auto& lc = cs.linear[i];
        const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                           [&](const auto& pair) { return free.count(pair.first) != 0; });
        if (covered >= 2) {
          result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                       ") not used to tighten the bound; result may be loose");
        }
      }
      return finish(complexity::evaluate(expr, capped), "pointwise-maxima-capped");
    }
  } else {
    // When count variable is not in expression, cost is per test case.
    perCase = std::make_shared<const Expr>(expr);
    result.assumptions.push_back("cost treated as per test case because '" + sums.front()->variable +
                                 "' is constrained across test cases ('" + countVar + "')");
  }

  // Evaluator closure for per-case cost under given bindings.
  auto evalPerCase = [&](const complexity::Bindings& b) {
    return monotone ? evalMonotoneWithConstraints(*perCase, b, ctx) : complexity::evaluate(*perCase, b);
  };

  // Step 4 (Case 1): Single sum constraint on a superadditive per-case function.
  // Greedy packing: pack as many full-size cases as possible, plus one partial remainder case.
  if (sums.size() == 1 && monotone && isSuperadditiveIn(*perCase, sums.front()->variable)) {
    const SumConstraint& sc = *sums.front();
    const double total = static_cast<double>(sc.maxTotal);
    const double perCaseMax = capped[sc.variable];
    const double fullCases = perCaseMax > 0.0 ? std::min(maxCases, std::floor(total / perCaseMax)) : 0.0;
    const double remainder = total - fullCases * perCaseMax;

    complexity::EvalResult full = evalPerCase(capped);
    if (!full.ok()) {
      return finish(full, "");
    }
    double value = fullCases * full.value;
    if (fullCases < maxCases && remainder > 0.0) {
      complexity::Bindings rest = capped;
      rest[sc.variable] = remainder;
      complexity::EvalResult partial = evalPerCase(rest);
      if (!partial.ok()) {
        return finish(partial, "");
      }
      value += partial.value;
    }
    // Check for unexploited product or linear constraints.
    for (size_t i = 0; i < cs.products.size(); ++i) {
      const auto& product = cs.products[i];
      const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                         [&](const std::string& v) { return free.count(v) != 0; });
      if (covered >= 2 && !ctx.productApplied[i]) {
        result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                     ") not used to tighten the bound; result may be loose");
      }
    }
    for (size_t i = 0; i < cs.linear.size(); ++i) {
      const auto& lc = cs.linear[i];
      const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                         [&](const auto& pair) { return free.count(pair.first) != 0; });
      if (covered >= 2 && !ctx.linearApplied[i]) {
        result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                     ") not used to tighten the bound; result may be loose");
      }
    }
    result.assumptions.push_back("worst case packs " + formatInt(static_cast<std::int64_t>(fullCases)) +
                                 " test case(s) of maximum size (superadditive per-case cost)");
    return finish(complexity::EvalResult{complexity::EvalStatus::Ok, value, {}}, "sum-constraint-packing");
  }

  // Step 5 (Case 2): Multiple sum constraints sharing one count variable on separable monotone terms.
  if (sums.size() > 1 && monotone) {
    // Collect individual additive terms from perCase expression.
    std::vector<ExprPtr> terms;
    std::function<void(const ExprPtr&)> collectTerms = [&](const ExprPtr& e) {
      if (e->kind() == ExprKind::Add) {
        for (const auto& op : e->operands()) {
          collectTerms(op);
        }
      } else {
        terms.push_back(e);
      }
    };
    collectTerms(perCase);

    // Verify whether each term references at most one sum-constrained variable.
    std::map<std::string, const SumConstraint*> sumByVar;
    for (const SumConstraint* sc : sums) {
      sumByVar[sc->variable] = sc;
    }

    bool allSeparable = true;
    for (const auto& term : terms) {
      const auto termFree = complexity::freeVariables(*term);
      size_t countSumVars = 0;
      for (const auto& v : termFree) {
        if (sumByVar.count(v) != 0) {
          ++countSumVars;
        }
      }
      if (countSumVars > 1) {
        allSeparable = false;
        break;
      }
    }

    // When all additive terms are separable, pack each term according to its own bound.
    if (allSeparable) {
      double totalValue = 0.0;
      bool evalOk = true;
      std::string evalError;
      bool anyPacked = false;

      for (const auto& term : terms) {
        const auto termFree = complexity::freeVariables(*term);
        const SumConstraint* sc = nullptr;
        for (const auto& v : termFree) {
          if (sumByVar.count(v) != 0) {
            sc = sumByVar[v];
            break;
          }
        }

        if (!sc) {
          // Term does not involve any sum-constrained variable; evaluate at capped bounds * maxCases.
          auto r = evalMonotoneWithConstraints(*term, capped, ctx);
          if (!r.ok()) {
            evalOk = false;
            evalError = r.detail;
            break;
          }
          totalValue += maxCases * r.value;
        } else {
          const double total = static_cast<double>(sc->maxTotal);
          const double perCaseMax = capped[sc->variable];
          if (isSuperadditiveIn(*term, sc->variable)) {
            // Pack superadditive term greedily.
            anyPacked = true;
            const double fullCases = perCaseMax > 0.0 ? std::min(maxCases, std::floor(total / perCaseMax)) : 0.0;
            const double remainder = total - fullCases * perCaseMax;

            auto full = evalMonotoneWithConstraints(*term, capped, ctx);
            if (!full.ok()) {
              evalOk = false;
              evalError = full.detail;
              break;
            }
            double termVal = fullCases * full.value;
            if (fullCases < maxCases && remainder > 0.0) {
              complexity::Bindings rest = capped;
              rest[sc->variable] = remainder;
              auto partial = evalMonotoneWithConstraints(*term, rest, ctx);
              if (!partial.ok()) {
                evalOk = false;
                evalError = partial.detail;
                break;
              }
              termVal += partial.value;
            }
            totalValue += termVal;
          } else {
            // Non-superadditive separable term: bound cases by total / min_bound.
            double termCases = maxCases;
            const auto& bound = cs.variables.find(sc->variable)->second;
            if (bound.min && *bound.min >= 1) {
              termCases = std::min(termCases, std::floor(total / static_cast<double>(*bound.min)));
            }
            auto r = evalMonotoneWithConstraints(*term, capped, ctx);
            if (!r.ok()) {
              evalOk = false;
              evalError = r.detail;
              break;
            }
            totalValue += termCases * r.value;
          }
        }
      }

      if (!evalOk) {
        return finish(complexity::EvalResult{complexity::EvalStatus::DomainError, 0.0, evalError}, "");
      }

      // Check for unexploited product or linear constraints.
      for (size_t i = 0; i < cs.products.size(); ++i) {
        const auto& product = cs.products[i];
        const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                           [&](const std::string& v) { return free.count(v) != 0; });
        if (covered >= 2 && !ctx.productApplied[i]) {
          result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                       ") not used to tighten the bound; result may be loose");
        }
      }
      for (size_t i = 0; i < cs.linear.size(); ++i) {
        const auto& lc = cs.linear[i];
        const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                           [&](const auto& pair) { return free.count(pair.first) != 0; });
        if (covered >= 2 && !ctx.linearApplied[i]) {
          result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                       ") not used to tighten the bound; result may be loose");
        }
      }

      std::string method = anyPacked ? "sum-constraint-packing" : "sum-constraint-cases";
      if (anyPacked) {
        result.assumptions.push_back(
            "worst case packs test case(s) of maximum size for sum-constrained variables (separable per-case cost)");
      } else {
        result.assumptions.push_back(
            "per-case cost not recognised as superadditive; bound is cases x per-case maximum");
      }
      return finish(complexity::EvalResult{complexity::EvalStatus::Ok, totalValue, {}}, std::move(method));
    }
  }

  // Step 6 (Case 3): Sound fallback for monotone costs: (number of cases) x (per-case maximum).
  double cases = maxCases;
  for (const SumConstraint* sc : sums) {
    const auto& bound = cs.variables.find(sc->variable)->second;
    if (bound.min && *bound.min >= 1) {
      cases = std::min(cases, std::floor(static_cast<double>(sc->maxTotal) / static_cast<double>(*bound.min)));
    }
  }
  complexity::EvalResult per = evalPerCase(capped);
  if (!per.ok()) {
    return finish(per, "");
  }
  for (size_t i = 0; i < cs.products.size(); ++i) {
    const auto& product = cs.products[i];
    const auto covered = std::count_if(product.variables.begin(), product.variables.end(),
                                       [&](const std::string& v) { return free.count(v) != 0; });
    if (covered >= 2 && !ctx.productApplied[i]) {
      result.assumptions.push_back("product constraint (bound " + formatInt(product.maxProduct) +
                                   ") not used to tighten the bound; result may be loose");
    }
  }
  for (size_t i = 0; i < cs.linear.size(); ++i) {
    const auto& lc = cs.linear[i];
    const auto covered = std::count_if(lc.coefficients.begin(), lc.coefficients.end(),
                                       [&](const auto& pair) { return free.count(pair.first) != 0; });
    if (covered >= 2 && !ctx.linearApplied[i]) {
      result.assumptions.push_back("linear constraint (bound " + formatInt(lc.bound) +
                                   ") not used to tighten the bound; result may be loose");
    }
  }
  result.assumptions.push_back("per-case cost not recognised as superadditive; bound is cases x per-case maximum");
  return finish(complexity::EvalResult{complexity::EvalStatus::Ok, cases * per.value, {}}, "sum-constraint-cases");
}

/// Evaluates the worst-case numerical workload of an asymptotic complexity expression
/// given variable bounds and constraints (linear bounds, product bounds, and test-case sum bounds).
/// Evaluates monotone non-decreasing expressions at upper bounds tightened by constraint relationships,
/// or packs test cases when superadditivity holds.
WorkloadResult evaluateWorkload(const Expr& expr, const ConstraintSet& cs) {
  WorkloadResult result;
  const std::set<std::string> free = complexity::freeVariables(expr);

  complexity::Bindings maxima;
  complexity::Bindings minima;
  bool negativeAllowed = false;
  for (const auto& name : free) {
    const auto it = cs.variables.find(name);
    if (it == cs.variables.end() || !it->second.max) {
      result.missing.push_back(name);
      continue;
    }
    maxima[name] = static_cast<double>(*it->second.max);
    minima[name] = (it->second.min && *it->second.min > 0) ? static_cast<double>(*it->second.min) : 0.0;
    if (it->second.min && *it->second.min < 0) {
      negativeAllowed = true;
    }
  }
  for (const auto& [name, bound] : cs.variables) {
    if (minima.find(name) == minima.end()) {
      minima[name] = (bound.min && *bound.min > 0) ? static_cast<double>(*bound.min) : 0.0;
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

  ConstraintContext ctx(cs.products, cs.linear, minima);

  if (sums.empty()) {
    return evaluateWorkloadWithoutSums(expr, cs, free, std::move(maxima), minima, monotone, status, ctx,
                                       std::move(result));
  }

  return evaluateWorkloadWithSums(expr, cs, free, sums, std::move(maxima), monotone, status, ctx, std::move(result));
}

}  // namespace bigolens::constraints
