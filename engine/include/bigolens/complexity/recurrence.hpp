#pragma once

#include <memory>
#include <optional>
#include <span>
#include <string>
#include <vector>

#include "bigolens/complexity/expression.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::complexity {

/// How one recursive call changes the size measure n of its invocation.
enum class SizeChange {
  Decrement,  ///< n - amount, amount > 0
  Divide,     ///< at most n / amount, amount >= 2
  Unknown     ///< not recognised (including non-decreasing arguments)
};

/// One self-call site, described without front-end types.
struct RecursiveCall {
  SizeChange change = SizeChange::Unknown;
  double amount = 0.0;
  std::string argument;     ///< The call's arguments as written in the source, for explanations.
  bool insideLoop = false;  ///< The call executes a data-dependent number of times per invocation.
  models::SourceLocation location;
};

/// A direct recurrence  T(n) = sum of T(call sizes) + work(n),  extracted from
/// the source by a front end. Solving it is the job of a RecurrenceSolver.
struct Recurrence {
  std::string function;
  /// The size measure in source terms, e.g. "n" or "hi - lo". Empty when no
  /// parameter (or parameter interval) shrinks at every call.
  std::string size;
  /// Approximations made during extraction (e.g. integer division treated as
  /// real division), reported with the finding.
  std::vector<std::string> assumptions;
  std::vector<RecursiveCall> calls;
  /// Largest number of self-calls executed along any single path through one
  /// invocation; empty when a self-call is inside a loop.
  std::optional<int> maxCallsPerInvocation;
  /// Non-recursive structural cost of one invocation, in terms of the size
  /// variable "n"; null when it could not be determined.
  ExprPtr work;
  std::vector<std::string> workLimitations;
};

/// Human-readable form, e.g. "T(n) = T(n - 1) + T(n - 2) + O(1)".
[[nodiscard]] std::string describe(const Recurrence& recurrence);

/// Symbol for the size measure: "n", unless the source size expression uses an
/// identifier of that name (e.g. "limit - n"), which would be ambiguous.
[[nodiscard]] std::string sizeSymbol(const Recurrence& recurrence);

struct RecurrenceSolution {
  models::BoundStatus status = models::BoundStatus::Unknown;
  ExprPtr time;        ///< Structural count of the whole recursion.
  std::string method;  ///< e.g. "master-theorem", "linear-decrement".
  std::vector<std::string> assumptions;
  std::vector<std::string> limitations;
};

/// Solves a family of recurrences. Implementations live in the core and must
/// not depend on any front end.
class RecurrenceSolver {
 public:
  RecurrenceSolver() = default;
  RecurrenceSolver(const RecurrenceSolver&) = delete;
  RecurrenceSolver& operator=(const RecurrenceSolver&) = delete;
  RecurrenceSolver(RecurrenceSolver&&) = delete;
  RecurrenceSolver& operator=(RecurrenceSolver&&) = delete;
  virtual ~RecurrenceSolver() = default;

  [[nodiscard]] virtual std::string name() const = 0;
  /// Returns a solution if the recurrence belongs to this solver's family.
  [[nodiscard]] virtual std::optional<RecurrenceSolution> solve(const Recurrence& recurrence) const = 0;
};

/// Solves recurrences of the master theorem form T(n) = a * T(n/b) + Theta(n^k (log n)^p).
class MasterTheoremSolver final : public RecurrenceSolver {
 public:
  [[nodiscard]] std::string name() const override { return "master-theorem"; }
  [[nodiscard]] std::optional<RecurrenceSolution> solve(const Recurrence& recurrence) const override;
};

/// Solves recurrences of the form T(n) = T(n - c) + f(n) by summation.
class LinearDecrementSolver final : public RecurrenceSolver {
 public:
  [[nodiscard]] std::string name() const override { return "linear-decrement"; }
  [[nodiscard]] std::optional<RecurrenceSolution> solve(const Recurrence& recurrence) const override;
};

/// Tries the solvers in order and returns the first solution, or nothing.
[[nodiscard]] std::optional<RecurrenceSolution> solveRecurrence(const Recurrence& recurrence,
                                                                std::span<const RecurrenceSolver* const> solvers);

/// Returns the standard solvers in precedence order (MasterTheorem, then LinearDecrement).
[[nodiscard]] std::vector<std::unique_ptr<RecurrenceSolver>> standardRecurrenceSolvers();

}  // namespace bigolens::complexity
