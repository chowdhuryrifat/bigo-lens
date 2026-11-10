#include "bigolens/complexity/recurrence.hpp"

#include <algorithm>
#include <cctype>
#include <cmath>
#include <map>
#include <sstream>
#include <utility>

#include "bigolens/complexity/asymptotic.hpp"

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

/// Symbol for the size measure: "n", unless the source size expression uses an
/// identifier of that name (e.g. "limit - n"), which would be ambiguous.
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

}  // namespace

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

}  // namespace bigolens::complexity
