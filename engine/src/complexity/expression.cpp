#include "bigolens/complexity/expression.hpp"

#include <cmath>
#include <sstream>
#include <stdexcept>
#include <utility>

namespace bigolens::complexity {

Expr::Expr(ExprKind kind, double number, std::string name, std::vector<ExprPtr> operands)
    : kind_(kind), number_(number), name_(std::move(name)), operands_(std::move(operands)) {}

namespace {

ExprPtr make(ExprKind kind, double number, std::string name, std::vector<ExprPtr> operands) {
  return std::make_shared<const Expr>(kind, number, std::move(name), std::move(operands));
}

void requireNonNull(const ExprPtr& expr, const char* what) {
  if (!expr) {
    throw std::invalid_argument(std::string("null expression passed to ") + what);
  }
}

}  // namespace

ExprPtr constant(double value) {
  if (!std::isfinite(value)) {
    throw std::invalid_argument("constant must be finite");
  }
  return make(ExprKind::Constant, value, {}, {});
}

ExprPtr variable(std::string name) {
  if (name.empty()) {
    throw std::invalid_argument("variable name must not be empty");
  }
  return make(ExprKind::Variable, 0.0, std::move(name), {});
}

bool isConstant(const Expr& expr, double value) noexcept {
  return expr.kind() == ExprKind::Constant && expr.number() == value;
}

ExprPtr add(std::vector<ExprPtr> terms) {
  std::vector<ExprPtr> flat;
  double constantSum = 0.0;
  for (auto& term : terms) {
    requireNonNull(term, "add");
    if (term->kind() == ExprKind::Add) {
      for (const auto& inner : term->operands()) {
        if (inner->kind() == ExprKind::Constant) {
          constantSum += inner->number();
        } else {
          flat.push_back(inner);
        }
      }
    } else if (term->kind() == ExprKind::Constant) {
      constantSum += term->number();
    } else {
      flat.push_back(std::move(term));
    }
  }
  if (constantSum != 0.0 || flat.empty()) {
    flat.push_back(constant(constantSum));
  }
  if (flat.size() == 1) {
    return flat.front();
  }
  return make(ExprKind::Add, 0.0, {}, std::move(flat));
}

ExprPtr subtract(ExprPtr lhs, const ExprPtr& rhs) {
  requireNonNull(rhs, "subtract");
  return add({std::move(lhs), multiply({constant(-1.0), rhs})});
}

ExprPtr multiply(std::vector<ExprPtr> factors) {
  std::vector<ExprPtr> flat;
  double constantProduct = 1.0;
  for (auto& factor : factors) {
    requireNonNull(factor, "multiply");
    if (factor->kind() == ExprKind::Multiply) {
      for (const auto& inner : factor->operands()) {
        if (inner->kind() == ExprKind::Constant) {
          constantProduct *= inner->number();
        } else {
          flat.push_back(inner);
        }
      }
    } else if (factor->kind() == ExprKind::Constant) {
      constantProduct *= factor->number();
    } else {
      flat.push_back(std::move(factor));
    }
  }
  if (constantProduct == 0.0) {
    return constant(0.0);
  }
  if (constantProduct != 1.0 || flat.empty()) {
    flat.insert(flat.begin(), constant(constantProduct));
  }
  if (flat.size() == 1) {
    return flat.front();
  }
  return make(ExprKind::Multiply, 0.0, {}, std::move(flat));
}

ExprPtr power(ExprPtr base, double exponent) {
  requireNonNull(base, "power");
  if (!std::isfinite(exponent) || exponent < 0.0) {
    throw std::invalid_argument("power exponent must be a finite non-negative constant");
  }
  if (exponent == 0.0) {
    return constant(1.0);
  }
  if (exponent == 1.0) {
    return base;
  }
  if (base->kind() == ExprKind::Constant) {
    return constant(std::pow(base->number(), exponent));
  }
  return make(ExprKind::Power, exponent, {}, {std::move(base)});
}

ExprPtr logarithm(ExprPtr argument, double base) {
  requireNonNull(argument, "logarithm");
  if (!std::isfinite(base) || base <= 1.0) {
    throw std::invalid_argument("logarithm base must be a finite constant greater than 1");
  }
  return make(ExprKind::Logarithm, base, {}, {std::move(argument)});
}

ExprPtr summation(std::string index, ExprPtr lower, ExprPtr upper, ExprPtr body) {
  requireNonNull(lower, "summation");
  requireNonNull(upper, "summation");
  requireNonNull(body, "summation");
  if (index.empty()) {
    throw std::invalid_argument("summation index must not be empty");
  }
  return make(ExprKind::Summation, 0.0, std::move(index), {std::move(lower), std::move(upper), std::move(body)});
}

ExprPtr maximum(std::vector<ExprPtr> operands) {
  if (operands.empty()) {
    throw std::invalid_argument("maximum requires at least one operand");
  }
  for (const auto& operand : operands) {
    requireNonNull(operand, "maximum");
  }
  if (operands.size() == 1) {
    return operands.front();
  }
  return make(ExprKind::Maximum, 0.0, {}, std::move(operands));
}

namespace {

void collectFree(const Expr& expr, std::set<std::string>& bound, std::set<std::string>& out) {
  switch (expr.kind()) {
    case ExprKind::Constant:
      return;
    case ExprKind::Variable:
      if (bound.count(expr.name()) == 0) {
        out.insert(expr.name());
      }
      return;
    case ExprKind::Summation: {
      collectFree(*expr.operands()[0], bound, out);
      collectFree(*expr.operands()[1], bound, out);
      const bool alreadyBound = bound.count(expr.name()) != 0;
      bound.insert(expr.name());
      collectFree(*expr.operands()[2], bound, out);
      if (!alreadyBound) {
        bound.erase(expr.name());
      }
      return;
    }
    default:
      for (const auto& operand : expr.operands()) {
        collectFree(*operand, bound, out);
      }
  }
}

std::string formatNumber(double value) {
  std::ostringstream stream;
  if (value == std::floor(value) && std::fabs(value) < 1e15) {
    stream << static_cast<long long>(value);
  } else {
    stream << value;
  }
  return stream.str();
}

bool needsParens(const Expr& expr) {
  return expr.kind() == ExprKind::Add || expr.kind() == ExprKind::Summation;
}

std::string render(const Expr& expr);

std::string renderFactor(const Expr& expr) {
  const std::string text = render(expr);
  return needsParens(expr) ? "(" + text + ")" : text;
}

std::string render(const Expr& expr) {
  switch (expr.kind()) {
    case ExprKind::Constant:
      return formatNumber(expr.number());
    case ExprKind::Variable:
      return expr.name();
    case ExprKind::Add: {
      std::string out;
      bool first = true;
      for (const auto& term : expr.operands()) {
        // Render "a + (-1 * b)" as "a - b" and "a + (-3)" as "a - 3".
        const bool negConst = term->kind() == ExprKind::Constant && term->number() < 0.0;
        const bool negProduct = term->kind() == ExprKind::Multiply &&
                                term->operands().front()->kind() == ExprKind::Constant &&
                                term->operands().front()->number() < 0.0;
        if (!first && (negConst || negProduct)) {
          if (negConst) {
            out += " - " + formatNumber(-term->number());
          } else {
            std::vector<ExprPtr> rest(term->operands().begin() + 1, term->operands().end());
            const double coefficient = -term->operands().front()->number();
            if (coefficient != 1.0) {
              rest.insert(rest.begin(), constant(coefficient));
            }
            out += " - " + renderFactor(*multiply(std::move(rest)));
          }
        } else {
          out += (first ? "" : " + ") + render(*term);
        }
        first = false;
      }
      return out;
    }
    case ExprKind::Multiply: {
      std::string out;
      for (std::size_t i = 0; i < expr.operands().size(); ++i) {
        out += (i == 0 ? "" : " * ") + renderFactor(*expr.operands()[i]);
      }
      return out;
    }
    case ExprKind::Power: {
      const Expr& base = *expr.operands().front();
      const bool simple = base.kind() == ExprKind::Variable || base.kind() == ExprKind::Constant;
      return (simple ? render(base) : "(" + render(base) + ")") + "^" + formatNumber(expr.number());
    }
    case ExprKind::Logarithm:
      return "log_" + formatNumber(expr.number()) + "(" + render(*expr.operands().front()) + ")";
    case ExprKind::Summation:
      return "sum_{" + expr.name() + "=" + render(*expr.operands()[0]) + "}^{" + render(*expr.operands()[1]) + "} " +
             renderFactor(*expr.operands()[2]);
    case ExprKind::Maximum: {
      std::string out = "max(";
      for (std::size_t i = 0; i < expr.operands().size(); ++i) {
        out += (i == 0 ? "" : ", ") + render(*expr.operands()[i]);
      }
      return out + ")";
    }
  }
  return "?";
}

}  // namespace

std::set<std::string> freeVariables(const Expr& expr) {
  std::set<std::string> bound;
  std::set<std::string> out;
  collectFree(expr, bound, out);
  return out;
}

std::string toString(const Expr& expr) {
  return render(expr);
}

}  // namespace bigolens::complexity
