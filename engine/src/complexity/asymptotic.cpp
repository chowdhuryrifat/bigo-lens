#include "bigolens/complexity/asymptotic.hpp"

#include <algorithm>
#include <cmath>
#include <sstream>
#include <tuple>
#include <utility>

namespace bigolens::complexity {
namespace {

using Terms = std::vector<Monomial>;

GrowthFactor factorOf(const Monomial& m, const std::string& var) {
  const auto it = m.find(var);
  return it == m.end() ? GrowthFactor{} : it->second;
}

bool atLeast(const GrowthFactor& a, const GrowthFactor& b) {
  return std::tie(a.power, a.logPower) >= std::tie(b.power, b.logPower);
}

/// a dominates b if a grows at least as fast in every variable.
bool dominates(const Monomial& a, const Monomial& b) {
  for (const auto& [var, factor] : b) {
    if (!atLeast(factorOf(a, var), factor)) {
      return false;
    }
  }
  for (const auto& [var, factor] : a) {
    if (!atLeast(factor, factorOf(b, var))) {
      return false;
    }
  }
  return true;
}

void cleanup(Monomial& m) {
  for (auto it = m.begin(); it != m.end();) {
    if (it->second.power == 0.0 && it->second.logPower == 0.0) {
      it = m.erase(it);
    } else {
      ++it;
    }
  }
}

Terms normalize(Terms terms) {
  for (auto& term : terms) {
    cleanup(term);
  }
  Terms out;
  for (std::size_t i = 0; i < terms.size(); ++i) {
    bool dominated = false;
    for (std::size_t j = 0; j < terms.size() && !dominated; ++j) {
      if (i == j) {
        continue;
      }
      if (dominates(terms[j], terms[i])) {
        // Equal monomials dominate each other; keep only the first occurrence.
        dominated = !(dominates(terms[i], terms[j]) && i < j);
      }
    }
    if (!dominated) {
      out.push_back(terms[i]);
    }
  }
  if (out.empty()) {
    out.emplace_back();
  }
  return out;
}

Terms multiplyTerms(const Terms& a, const Terms& b) {
  Terms out;
  for (const auto& x : a) {
    for (const auto& y : b) {
      Monomial product = x;
      for (const auto& [var, factor] : y) {
        auto& target = product[var];
        target.power += factor.power;
        target.logPower += factor.logPower;
      }
      out.push_back(std::move(product));
    }
  }
  return normalize(std::move(out));
}

Terms powerTerms(const Terms& a, double k) {
  if (k == 0.0) {
    return {Monomial{}};
  }
  Terms out;
  for (auto m : a) {
    for (auto& [var, factor] : m) {
      factor.power *= k;
      factor.logPower *= k;
    }
    out.push_back(std::move(m));
  }
  return normalize(std::move(out));
}

/// log(sum of monomials) = Theta(sum of log v over variables with positive power).
std::optional<Terms> logTerms(const Terms& a) {
  Terms out;
  for (const auto& m : a) {
    for (const auto& [var, factor] : m) {
      if (factor.power > 0.0) {
        out.push_back(Monomial{{var, GrowthFactor{0.0, 1.0}}});
      } else if (factor.logPower > 0.0) {
        return std::nullopt;  // log log v is not representable
      }
    }
  }
  return normalize(std::move(out));
}

bool isNegativeTerm(const Expr& term) {
  if (term.kind() == ExprKind::Constant) {
    return term.number() <= 0.0;
  }
  if (term.kind() == ExprKind::Multiply) {
    const Expr& lead = *term.operands().front();
    return lead.kind() == ExprKind::Constant && lead.number() < 0.0;
  }
  return false;
}

std::optional<Terms> derive(const Expr& expr);

/// Replace occurrences of `var` (a summation index) by the bound `with`.
std::optional<Terms> substitute(const Terms& terms, const std::string& var, const Terms& with) {
  Terms out;
  for (const auto& m : terms) {
    const GrowthFactor f = factorOf(m, var);
    Monomial rest = m;
    rest.erase(var);
    Terms piece{rest};
    if (f.power > 0.0) {
      piece = multiplyTerms(piece, powerTerms(with, f.power));
    }
    if (f.logPower > 0.0) {
      auto logs = logTerms(with);
      if (!logs) {
        return std::nullopt;
      }
      piece = multiplyTerms(piece, powerTerms(*logs, f.logPower));
    }
    out.insert(out.end(), piece.begin(), piece.end());
  }
  return normalize(std::move(out));
}

std::optional<Terms> derive(const Expr& expr) {
  switch (expr.kind()) {
    case ExprKind::Constant:
      return Terms{Monomial{}};
    case ExprKind::Variable:
      return Terms{Monomial{{expr.name(), GrowthFactor{1.0, 0.0}}}};
    case ExprKind::Add:
    case ExprKind::Maximum: {
      Terms out;
      for (const auto& operand : expr.operands()) {
        if (expr.kind() == ExprKind::Add && isNegativeTerm(*operand)) {
          continue;  // dropping negative terms keeps an upper bound
        }
        auto sub = derive(*operand);
        if (!sub) {
          return std::nullopt;
        }
        out.insert(out.end(), sub->begin(), sub->end());
      }
      return normalize(std::move(out));
    }
    case ExprKind::Multiply: {
      Terms acc{Monomial{}};
      for (const auto& factor : expr.operands()) {
        if (factor->kind() == ExprKind::Constant) {
          continue;
        }
        auto sub = derive(*factor);
        if (!sub) {
          return std::nullopt;
        }
        acc = multiplyTerms(acc, *sub);
      }
      return acc;
    }
    case ExprKind::Power: {
      auto base = derive(*expr.operands().front());
      if (!base) {
        return std::nullopt;
      }
      return powerTerms(*base, expr.number());
    }
    case ExprKind::Logarithm: {
      auto arg = derive(*expr.operands().front());
      if (!arg) {
        return std::nullopt;
      }
      return logTerms(*arg);
    }
    case ExprKind::Summation: {
      // count <= upper bound, and every term is bounded by the body at i = upper.
      auto upper = derive(*expr.operands()[1]);
      auto body = derive(*expr.operands()[2]);
      if (!upper || !body) {
        return std::nullopt;
      }
      auto bodyAtUpper = substitute(*body, expr.name(), *upper);
      if (!bodyAtUpper) {
        return std::nullopt;
      }
      return multiplyTerms(*upper, *bodyAtUpper);
    }
  }
  return std::nullopt;
}

std::string formatExponent(double value) {
  std::ostringstream stream;
  if (value == std::floor(value)) {
    stream << static_cast<long long>(value);
  } else {
    stream << value;
  }
  return stream.str();
}

std::string renderMonomial(const Monomial& m) {
  if (m.empty()) {
    return "1";
  }
  std::vector<std::string> powers;
  std::vector<std::string> logs;
  for (const auto& [var, factor] : m) {
    if (factor.power == 1.0) {
      powers.push_back(var);
    } else if (factor.power > 0.0) {
      powers.push_back(var + "^" + formatExponent(factor.power));
    }
    if (factor.logPower == 1.0) {
      logs.push_back("log " + var);
    } else if (factor.logPower > 0.0) {
      logs.push_back("log^" + formatExponent(factor.logPower) + " " + var);
    }
  }
  std::string out;
  for (const auto& part : powers) {
    out += (out.empty() ? "" : " ") + part;
  }
  for (const auto& part : logs) {
    out += (out.empty() ? "" : " ") + part;
  }
  return out;
}

std::pair<double, double> weight(const Monomial& m) {
  double power = 0.0;
  double logPower = 0.0;
  for (const auto& [var, factor] : m) {
    power += factor.power;
    logPower += factor.logPower;
  }
  return {power, logPower};
}

}  // namespace

std::optional<AsymptoticBound> asymptoticUpperBound(const Expr& expr) {
  auto terms = derive(expr);
  if (!terms) {
    return std::nullopt;
  }
  return AsymptoticBound{*terms};
}

std::string toString(const AsymptoticBound& bound) {
  std::vector<Monomial> terms = bound.terms;
  std::sort(terms.begin(), terms.end(), [](const Monomial& a, const Monomial& b) {
    const auto wa = weight(a);
    const auto wb = weight(b);
    if (wa != wb) {
      return wa > wb;
    }
    return renderMonomial(a) < renderMonomial(b);
  });
  std::string out = "O(";
  for (std::size_t i = 0; i < terms.size(); ++i) {
    out += (i == 0 ? "" : " + ") + renderMonomial(terms[i]);
  }
  if (terms.empty()) {
    out += "1";
  }
  return out + ")";
}

}  // namespace bigolens::complexity
