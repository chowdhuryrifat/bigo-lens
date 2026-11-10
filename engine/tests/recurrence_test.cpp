#include "bigolens/complexity/recurrence.hpp"

#include <gtest/gtest.h>

#include <vector>

#include "bigolens/analyzers/recurrence_resolution.hpp"
#include "bigolens/complexity/expression.hpp"

using namespace bigolens;
using complexity::Recurrence;
using complexity::RecursiveCall;
using complexity::SizeChange;

namespace {

RecursiveCall call(SizeChange change, double amount) {
  RecursiveCall c;
  c.change = change;
  c.amount = amount;
  return c;
}

Recurrence fibonacci() {
  Recurrence r;
  r.function = "fib";
  r.size = "n";
  r.calls = {call(SizeChange::Decrement, 1), call(SizeChange::Decrement, 2)};
  r.maxCallsPerInvocation = 2;
  r.work = complexity::constant(1);
  return r;
}

class FixedSolver final : public complexity::RecurrenceSolver {
 public:
  explicit FixedSolver(bool applies) : applies_(applies) {}
  std::string name() const override { return applies_ ? "fixed" : "never"; }
  std::optional<complexity::RecurrenceSolution> solve(const Recurrence&) const override {
    if (!applies_) {
      return std::nullopt;
    }
    complexity::RecurrenceSolution solution;
    solution.status = models::BoundStatus::Estimated;
    solution.time = complexity::variable("n");
    solution.assumptions.push_back("test assumption");
    return solution;
  }

 private:
  bool applies_;
};

models::Finding recursionFinding(Recurrence recurrence) {
  models::Finding finding;
  finding.category = models::FindingCategory::Recursion;
  finding.boundStatus = models::BoundStatus::Unsupported;
  finding.confidence = models::Confidence::None;
  finding.provenance = "clang-recursion-detector";
  finding.recurrence = std::move(recurrence);
  return finding;
}

}  // namespace

TEST(Recurrence, DescribesFibonacci) {
  EXPECT_EQ(complexity::describe(fibonacci()), "T(n) = T(n - 1) + T(n - 2) + O(1)");
}

TEST(Recurrence, GroupsIdenticalCallsAndRendersWorkAsymptotically) {
  Recurrence r;
  r.size = "hi - lo";
  r.calls = {call(SizeChange::Divide, 2), call(SizeChange::Divide, 2)};
  r.maxCallsPerInvocation = 2;
  r.work = complexity::add({complexity::variable("n"), complexity::constant(1)});
  EXPECT_EQ(complexity::describe(r), "T(n) = 2 T(n/2) + O(n), where n = hi - lo");
}

TEST(Recurrence, ExclusiveCallSitesAreListedAsAlternatives) {
  Recurrence r;
  r.size = "hi - lo";
  r.calls = {call(SizeChange::Divide, 2), call(SizeChange::Divide, 2)};
  r.maxCallsPerInvocation = 1;
  r.work = complexity::constant(1);
  EXPECT_EQ(complexity::describe(r), "T(n) = at most 1 of {T(n/2), T(n/2)} + O(1), where n = hi - lo");
}

TEST(Recurrence, SizeSymbolAvoidsSourceIdentifiers) {
  Recurrence r;
  r.size = "limit - n";
  r.calls = {call(SizeChange::Decrement, 1)};
  r.maxCallsPerInvocation = 1;
  r.work = complexity::variable("n");
  EXPECT_EQ(complexity::describe(r), "T(m) = T(m - 1) + O(m), where m = limit - n");
}

TEST(Recurrence, UnknownShapesAreExplained) {
  Recurrence r;
  RecursiveCall inLoop = call(SizeChange::Unknown, 0);
  inLoop.insideLoop = true;
  r.calls = {inLoop};
  EXPECT_EQ(complexity::describe(r),
            "T(n) = (calls inside a loop: T(?)) + f(n) (unknown), with no parameter that shrinks at every call");
}

TEST(Recurrence, SolversAreTriedInOrder) {
  const FixedSolver never(false);
  const FixedSolver fixed(true);
  std::vector<const complexity::RecurrenceSolver*> solvers{&never, &fixed};
  const auto solution = complexity::solveRecurrence(fibonacci(), solvers);
  ASSERT_TRUE(solution.has_value());
  EXPECT_EQ(solution->method, "fixed");
  EXPECT_FALSE(complexity::solveRecurrence(fibonacci(), {}).has_value());
}

TEST(RecurrenceResolution, WithoutSolversFindingStaysUnsupportedAndRecordsTheRecurrence) {
  std::vector<models::Finding> findings{recursionFinding(fibonacci())};
  analyzers::resolveRecurrences(findings, {});
  const auto& f = findings.front();
  EXPECT_EQ(f.boundStatus, models::BoundStatus::Unsupported);
  EXPECT_EQ(f.confidence, models::Confidence::None);
  EXPECT_EQ(f.timeExpression, nullptr);
  ASSERT_GE(f.limitations.size(), 2U);
  EXPECT_EQ(f.limitations[0], "extracted recurrence: T(n) = T(n - 1) + T(n - 2) + O(1)");
  EXPECT_EQ(f.limitations[1], "recurrence solving is not implemented yet; no bound is reported");
}

TEST(RecurrenceResolution, ApplicableSolverSetsTheBound) {
  std::vector<models::Finding> findings{recursionFinding(fibonacci())};
  const FixedSolver fixed(true);
  std::vector<const complexity::RecurrenceSolver*> solvers{&fixed};
  analyzers::resolveRecurrences(findings, solvers);
  const auto& f = findings.front();
  EXPECT_EQ(f.boundStatus, models::BoundStatus::Estimated);
  EXPECT_EQ(f.confidence, models::Confidence::Low);
  ASSERT_NE(f.timeExpression, nullptr);
  EXPECT_EQ(complexity::toString(*f.timeExpression), "n");
  EXPECT_EQ(f.provenance, "clang-recursion-detector+fixed");
  EXPECT_EQ(f.assumptions.front(), "extracted recurrence: T(n) = T(n - 1) + T(n - 2) + O(1)");
}

TEST(RecurrenceResolution, FindingsWithoutRecurrencesAreUntouched) {
  models::Finding loop;
  loop.category = models::FindingCategory::LoopNest;
  loop.boundStatus = models::BoundStatus::Proven;
  std::vector<models::Finding> findings{loop};
  analyzers::resolveRecurrences(findings, {});
  EXPECT_EQ(findings.front().boundStatus, models::BoundStatus::Proven);
  EXPECT_TRUE(findings.front().limitations.empty());
}
