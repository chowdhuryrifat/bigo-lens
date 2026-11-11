#include "bigolens/analyzers/recurrence_resolution.hpp"

#include <algorithm>
#include <string>

namespace bigolens::analyzers {
namespace {

void appendUnique(std::vector<std::string>& into, const std::vector<std::string>& from) {
  for (const auto& item : from) {
    if (std::find(into.begin(), into.end(), item) == into.end()) {
      into.push_back(item);
    }
  }
}

}  // namespace

void resolveRecurrences(std::vector<models::Finding>& findings,
                        std::span<const complexity::RecurrenceSolver* const> solvers) {
  for (auto& finding : findings) {
    if (!finding.recurrence) {
      continue;
    }
    const complexity::Recurrence& recurrence = *finding.recurrence;
    const std::string extracted = "extracted recurrence: " + complexity::describe(recurrence);
    std::vector<std::string> notes = recurrence.assumptions;
    appendUnique(notes, recurrence.workLimitations);

    if (auto solution = complexity::solveRecurrence(recurrence, solvers)) {
      finding.timeExpression = solution->time;
      finding.boundStatus = solution->time ? solution->status : models::BoundStatus::Unknown;
      finding.assumptions.push_back(extracted);
      appendUnique(finding.assumptions, solution->assumptions);
      appendUnique(finding.assumptions, notes);
      appendUnique(finding.limitations, solution->limitations);
      finding.provenance += "+" + solution->method;
      switch (finding.boundStatus) {
        case models::BoundStatus::Proven:
          finding.confidence = finding.limitations.empty() ? models::Confidence::High : models::Confidence::Medium;
          break;
        case models::BoundStatus::Estimated:
          finding.confidence = models::Confidence::Low;
          break;
        case models::BoundStatus::Unknown:
        case models::BoundStatus::Unsupported:
          finding.confidence = models::Confidence::None;
          break;
      }
      continue;
    }

    bool multipleSubtractive = false;
    if (recurrence.maxCallsPerInvocation.value_or(0) > 1 && !recurrence.calls.empty()) {
      multipleSubtractive =
          std::all_of(recurrence.calls.begin(), recurrence.calls.end(),
                      [](const complexity::RecursiveCall& c) { return c.change == complexity::SizeChange::Decrement; });
    }

    finding.limitations.insert(finding.limitations.begin(), extracted);
    finding.limitations.push_back(
        solvers.empty()
            ? "recurrence solving is not implemented yet; no bound is reported"
            : (multipleSubtractive ? "multiple recursion with subtractive arguments is not supported; exponential "
                                     "bounds are not derived"
                                   : "no recurrence solver applies to this recurrence; no bound is reported"));
    appendUnique(finding.limitations, notes);
  }
}

}  // namespace bigolens::analyzers
