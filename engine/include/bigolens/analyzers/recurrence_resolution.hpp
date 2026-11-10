#pragma once

#include <span>
#include <vector>

#include "bigolens/complexity/recurrence.hpp"
#include "bigolens/models/finding.hpp"

namespace bigolens::analyzers {

/// For every finding carrying an extracted recurrence: if a solver applies,
/// sets the finding's time expression, status, confidence and provenance from
/// the solution; otherwise keeps it Unsupported and records the recurrence
/// and the reason in its limitations. Front-end independent.
void resolveRecurrences(std::vector<models::Finding>& findings,
                        std::span<const complexity::RecurrenceSolver* const> solvers);

}  // namespace bigolens::analyzers
