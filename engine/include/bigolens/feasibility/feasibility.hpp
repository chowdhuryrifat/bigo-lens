#pragma once

#include <cstdint>
#include <optional>
#include <string>

#include "bigolens/estimation/memory_estimator.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::feasibility {

/// Runtime range in milliseconds: low <= central <= high.
struct RuntimeEstimate {
  double lowMs = 0.0;
  double centralMs = 0.0;
  double highMs = 0.0;
};

/// Policy thresholds. These are product policy, not physical facts.
struct FeasibilityPolicy {
  /// "Likely safe" requires the pessimistic estimate to be at most this fraction of the limit.
  double timeSafeFraction = 0.5;
  /// "Likely safe" requires a known memory upper bound at most this fraction of the limit.
  double memorySafeFraction = 0.8;
};

struct FeasibilityAssessment {
  models::FeasibilityStatus status = models::FeasibilityStatus::Unknown;
  std::string reason;
};

/// Classifies a runtime range against a time limit. Unknown without both.
[[nodiscard]] FeasibilityAssessment assessTime(const std::optional<RuntimeEstimate>& estimate,
                                               std::optional<std::int64_t> timeLimitMs,
                                               const FeasibilityPolicy& policy = {});

/// Classifies memory. Only HighRisk can be concluded from lower bounds; safety
/// requires an upper bound, which static analysis does not provide yet.
[[nodiscard]] FeasibilityAssessment assessMemory(const estimation::MemoryEstimate& estimate,
                                                 std::optional<double> knownUpperBoundBytes,
                                                 std::optional<std::int64_t> memoryLimitBytes,
                                                 const FeasibilityPolicy& policy = {});

}  // namespace bigolens::feasibility
