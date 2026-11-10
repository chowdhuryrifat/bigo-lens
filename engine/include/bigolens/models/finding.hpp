#pragma once

#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "bigolens/complexity/expression.hpp"
#include "bigolens/complexity/recurrence.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::models {

enum class FindingCategory {
  LoopNest,             ///< A top-level loop nest inside a function.
  Recursion,            ///< A recursive function.
  StaticStorage,        ///< Global or static-duration storage.
  DynamicAllocation,    ///< Heap allocations such as sized std::vector construction.
  UnsupportedConstruct  ///< A construct the analyzer recognises but cannot model.
};

[[nodiscard]] std::string_view toString(FindingCategory category) noexcept;

/// A single explainable analysis result. All symbolic quantities are expression
/// trees; strings are derived for presentation only.
struct Finding {
  std::string id;
  FindingCategory category = FindingCategory::UnsupportedConstruct;
  std::string pattern;  ///< Machine-readable detected pattern, e.g. "nested-loop".
  std::string description;
  std::string functionName;
  SourceLocation location;

  /// Structural count of executions of straight-line statement groups (time).
  complexity::ExprPtr timeExpression;
  /// Bytes attributable to this finding (space).
  complexity::ExprPtr spaceExpression;

  BoundStatus boundStatus = BoundStatus::Unknown;
  Confidence confidence = Confidence::None;
  std::vector<std::string> assumptions;
  std::vector<std::string> limitations;
  std::string provenance;  ///< Which analyzer produced the finding.

  /// Recurrence extracted for Recursion findings; resolved (solved or
  /// explained) by analyzers::resolveRecurrences before serialisation.
  std::optional<complexity::Recurrence> recurrence;
};

}  // namespace bigolens::models
