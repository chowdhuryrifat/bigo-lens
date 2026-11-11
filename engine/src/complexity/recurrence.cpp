#include "bigolens/complexity/recurrence.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <map>
#include <memory>
#include <sstream>
#include <utility>

#include "bigolens/complexity/asymptotic.hpp"
#include "bigolens/complexity/expression.hpp"

namespace bigolens::complexity {
namespace {

std::string formatAmount(double value) {
  std::ostringstream out;
  out << value;
  return out.str();
}

std::string callTerm(const RecursiveCall& call, const std::string& symbol) {
  switch (call.change) {
    case SizeChange::Decrement:
      return "T(" + symbol + " - " + formatAmount(call.amount) + ")";
    case SizeChange::Divide:
      return "T(" + symbol + "/" + formatAmount(call.amount) + ")";
    case SizeChange::Unknown:
      return "T(?)";
  }
  return "T(?)";
}

std::string workTerm(const Recurrence& recurrence, const std::string& symbol) {
  if (!recurrence.work) {
    return "f(" + symbol + ") (unknown)";
  }
  const ExprPtr work = symbol == "n" ? recurrence.work : substitute(recurrence.work, "n", variable(symbol));
  if (auto bound = asymptoticUpperBound(*work)) {
    return toString(*bound);
  }
  return toString(*work);
}

bool containsIdentifier(const std::string& text, const std::string& identifier) {
  auto isIdent = [](char c) { return std::isalnum(static_cast<unsigned char>(c)) != 0 || c == '_'; };
  for (std::size_t pos = text.find(identifier); pos != std::string::npos; pos = text.find(identifier, pos + 1)) {
    const bool startOk = pos == 0 || !isIdent(text[pos - 1]);
    const std::size_t after = pos + identifier.size();
    const bool endOk = after >= text.size() || !isIdent(text[after]);
    if (startOk && endOk) {
      return true;
    }
  }
  return false;
}

struct AsymptoticTerm {
  double power = 0.0;
  double logPower = 0.0;
};

std::optional<AsymptoticTerm> extractTerm(const Expr& work, const std::string& symbol) {
  auto bound = asymptoticUpperBound(work);
  if (!bound) {
    return std::nullopt;
  }
  const auto& terms = bound->terms;
  if (terms.empty()) {
    return AsymptoticTerm{0.0, 0.0};
  }
  if (terms.size() == 1) {
    const auto& m = terms[0];
    if (m.empty()) {
      return AsymptoticTerm{0.0, 0.0};
    }
    if (m.size() == 1 && m.count(symbol) == 1) {
      const auto& factor = m.at(symbol);
      if (factor.power >= 0.0 && factor.logPower >= 0.0) {
        return AsymptoticTerm{factor.power, factor.logPower};
      }
    }
  }
  return std::nullopt;
}

}  // namespace

std::string sizeSymbol(const Recurrence& recurrence) {
  if (recurrence.size.empty() || recurrence.size == "n") {
    return "n";
  }
  for (const char* candidate : {"n", "m", "s", "k", "len"}) {
    if (!containsIdentifier(recurrence.size, candidate)) {
      return candidate;
    }
  }
  return "size";
}

std::string describe(const Recurrence& recurrence) {
  const std::string symbol = sizeSymbol(recurrence);
  const auto sites = static_cast<int>(recurrence.calls.size());
  const bool anyLoop = std::any_of(recurrence.calls.begin(), recurrence.calls.end(),
                                   [](const RecursiveCall& call) { return call.insideLoop; });
  const bool exclusive = !anyLoop && recurrence.maxCallsPerInvocation && *recurrence.maxCallsPerInvocation < sites;

  std::string calls;
  if (exclusive) {
    // Alternatives are listed one per call site: "at most 1 of {T(n/2), T(n/2)}".
    for (const auto& call : recurrence.calls) {
      calls += (calls.empty() ? "" : ", ") + callTerm(call, symbol);
    }
    calls = "at most " + std::to_string(*recurrence.maxCallsPerInvocation) + " of {" + calls + "}";
  } else {
    // Identical call shapes are grouped in first-appearance order: "2 T(n/2)".
    std::vector<std::pair<std::string, int>> groups;
    for (const auto& call : recurrence.calls) {
      const std::string term = callTerm(call, symbol);
      auto it = std::find_if(groups.begin(), groups.end(), [&](const auto& g) { return g.first == term; });
      if (it == groups.end()) {
        groups.emplace_back(term, 1);
      } else {
        ++it->second;
      }
    }
    for (const auto& [term, count] : groups) {
      calls += (calls.empty() ? "" : " + ") + (count > 1 ? std::to_string(count) + " " : std::string()) + term;
    }
    if (anyLoop) {
      calls = "(calls inside a loop: " + calls + ")";
    }
  }
  std::string text = "T(" + symbol + ") = " + calls + " + " + workTerm(recurrence, symbol);
  if (recurrence.size.empty()) {
    text += ", with no parameter that shrinks at every call";
  } else if (recurrence.size != symbol) {
    text += ", where " + symbol + " = " + recurrence.size;
  }
  return text;
}

std::optional<RecurrenceSolution> solveRecurrence(const Recurrence& recurrence,
                                                  std::span<const RecurrenceSolver* const> solvers) {
  for (const RecurrenceSolver* solver : solvers) {
    if (solver == nullptr) {
      continue;
    }
    if (auto solution = solver->solve(recurrence)) {
      if (solution->method.empty()) {
        solution->method = solver->name();
      }
      return solution;
    }
  }
  return std::nullopt;
}

std::optional<RecurrenceSolution> MasterTheoremSolver::solve(const Recurrence& recurrence) const {
  if (recurrence.calls.empty() || !recurrence.work || !recurrence.maxCallsPerInvocation.has_value()) {
    return std::nullopt;
  }
  for (const auto& call : recurrence.calls) {
    if (call.insideLoop || call.change != SizeChange::Divide) {
      return std::nullopt;
    }
  }
  const double b = recurrence.calls.front().amount;
  if (b < 2.0 - 1e-9) {
    return std::nullopt;
  }
  for (const auto& call : recurrence.calls) {
    if (std::fabs(call.amount - b) > 1e-6) {
      return std::nullopt;
    }
  }
  const int a = *recurrence.maxCallsPerInvocation;
  if (a < 1) {
    return std::nullopt;
  }

  const std::string symbol = sizeSymbol(recurrence);
  const ExprPtr work = symbol == "n" ? recurrence.work : substitute(recurrence.work, "n", variable(symbol));
  auto term = extractTerm(*work, symbol);
  if (!term) {
    return std::nullopt;
  }
  const double k = term->power;
  const double p = term->logPower;

  const double cCrit = std::log(static_cast<double>(a)) / std::log(b);

  bool isCase2 = false;
  if (std::fabs(k - std::round(k)) < 1e-6) {
    const int kInt = static_cast<int>(std::round(k));
    if (kInt >= 0 && kInt <= 30) {
      const double bk = std::pow(b, kInt);
      if (std::fabs(static_cast<double>(a) - bk) < 1e-6) {
        isCase2 = true;
      }
    }
  }

  ExprPtr timeExpr;
  if (isCase2) {
    // Case 2: a = b^k => T(n) = Theta(n^k (log n)^(p + 1))
    const ExprPtr logTerm = add({logarithm(variable(symbol), b), constant(1.0)});
    if (std::fabs(k) < 1e-6 && std::fabs(p) < 1e-6) {
      timeExpr = logTerm;
    } else if (std::fabs(k - 1.0) < 1e-6 && std::fabs(p) < 1e-6) {
      timeExpr = multiply({variable(symbol), logTerm});
    } else if (std::fabs(k) >= 1e-6 && std::fabs(p) < 1e-6) {
      timeExpr = multiply({power(variable(symbol), k), logTerm});
    } else if (std::fabs(k) < 1e-6 && std::fabs(p) >= 1e-6) {
      timeExpr = power(logTerm, p + 1.0);
    } else {
      timeExpr = multiply({power(variable(symbol), k), power(logTerm, p + 1.0)});
    }
  } else if (static_cast<double>(a) > std::pow(b, k) + 1e-6) {
    // Case 1: a > b^k => T(n) = Theta(n^(log_b a))
    if (std::fabs(cCrit - 1.0) < 1e-6) {
      timeExpr = variable(symbol);
    } else if (std::fabs(cCrit) < 1e-6) {
      timeExpr = constant(1.0);
    } else {
      const double expVal = std::fabs(cCrit - std::round(cCrit)) < 1e-6 ? std::round(cCrit) : cCrit;
      timeExpr = power(variable(symbol), expVal);
    }
  } else {
    // Case 3: a < b^k => T(n) = Theta(n^k (log n)^p)
    if (std::fabs(p) < 1e-6) {
      if (std::fabs(k - 1.0) < 1e-6) {
        timeExpr = variable(symbol);
      } else if (std::fabs(k) < 1e-6) {
        timeExpr = constant(1.0);
      } else {
        timeExpr = power(variable(symbol), k);
      }
    } else {
      const ExprPtr logTerm = add({logarithm(variable(symbol), b), constant(1.0)});
      if (std::fabs(k - 1.0) < 1e-6) {
        timeExpr = multiply({variable(symbol), power(logTerm, p)});
      } else if (std::fabs(k) < 1e-6) {
        timeExpr = power(logTerm, p);
      } else {
        timeExpr = multiply({power(variable(symbol), k), power(logTerm, p)});
      }
    }
  }

  RecurrenceSolution solution;
  solution.status = models::BoundStatus::Proven;
  solution.time = timeExpr;
  solution.method = "master-theorem";
  solution.assumptions.push_back("base case assumed to require O(1) time");
  return solution;
}

std::optional<RecurrenceSolution> LinearDecrementSolver::solve(const Recurrence& recurrence) const {
  if (recurrence.calls.empty() || !recurrence.work || !recurrence.maxCallsPerInvocation.has_value()) {
    return std::nullopt;
  }
  if (*recurrence.maxCallsPerInvocation != 1) {
    return std::nullopt;
  }
  for (const auto& call : recurrence.calls) {
    if (call.insideLoop || call.change != SizeChange::Decrement) {
      return std::nullopt;
    }
  }
  const double c = recurrence.calls.front().amount;
  if (c < 1.0 - 1e-9) {
    return std::nullopt;
  }
  for (const auto& call : recurrence.calls) {
    if (std::fabs(call.amount - c) > 1e-6) {
      return std::nullopt;
    }
  }

  const std::string symbol = sizeSymbol(recurrence);
  const ExprPtr work = symbol == "n" ? recurrence.work : substitute(recurrence.work, "n", variable(symbol));
  auto term = extractTerm(*work, symbol);
  if (!term) {
    return std::nullopt;
  }
  const double k = term->power;
  const double p = term->logPower;

  ExprPtr timeExpr;
  if (std::fabs(k) < 1e-6 && std::fabs(p) < 1e-6) {
    timeExpr = std::fabs(c - 1.0) < 1e-6 ? variable(symbol) : multiply({constant(1.0 / c), variable(symbol)});
  } else if (std::fabs(k - 1.0) < 1e-6 && std::fabs(p) < 1e-6) {
    if (std::fabs(c - 1.0) < 1e-6) {
      timeExpr = summation("i", constant(1.0), variable(symbol), variable("i"));
    } else {
      timeExpr = multiply({constant(1.0 / (2.0 * c)), power(variable(symbol), 2.0)});
    }
  } else if (std::fabs(p) < 1e-6) {
    timeExpr = power(variable(symbol), k + 1.0);
  } else {
    timeExpr = multiply({power(variable(symbol), k + 1.0), power(logarithm(variable(symbol)), p)});
  }

  RecurrenceSolution solution;
  solution.status = models::BoundStatus::Proven;
  solution.time = timeExpr;
  solution.method = "linear-decrement";
  solution.assumptions.push_back("base case assumed to require O(1) time");
  return solution;
}

std::vector<std::unique_ptr<RecurrenceSolver>> standardRecurrenceSolvers() {
  std::vector<std::unique_ptr<RecurrenceSolver>> solvers;
  solvers.push_back(std::make_unique<MasterTheoremSolver>());
  solvers.push_back(std::make_unique<LinearDecrementSolver>());
  return solvers;
}

}  // namespace bigolens::complexity
