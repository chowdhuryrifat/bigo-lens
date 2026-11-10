#include "bigolens/feasibility/feasibility.hpp"

#include <sstream>

namespace bigolens::feasibility {

using models::FeasibilityStatus;

namespace {
std::string mb(double bytes) {
  std::ostringstream stream;
  stream.precision(4);
  stream << bytes / (1024.0 * 1024.0) << " MiB";
  return stream.str();
}
}  // namespace

FeasibilityAssessment assessTime(const std::optional<RuntimeEstimate>& estimate,
                                 std::optional<std::int64_t> timeLimitMs, const FeasibilityPolicy& policy) {
  if (!timeLimitMs) {
    return {FeasibilityStatus::Unknown, "no time limit configured"};
  }
  if (!estimate) {
    return {FeasibilityStatus::Unknown,
            "no runtime estimate available; structural workload alone does not determine runtime"};
  }
  const double limit = static_cast<double>(*timeLimitMs);
  if (estimate->lowMs > limit) {
    return {FeasibilityStatus::HighRisk, "even the optimistic runtime estimate exceeds the time limit"};
  }
  if (estimate->highMs <= policy.timeSafeFraction * limit) {
    return {FeasibilityStatus::LikelySafe, "the pessimistic runtime estimate is well within the time limit"};
  }
  return {FeasibilityStatus::Borderline, "the runtime range overlaps the region near the time limit"};
}

FeasibilityAssessment assessMemory(const estimation::MemoryEstimate& estimate,
                                   std::optional<double> knownUpperBoundBytes,
                                   std::optional<std::int64_t> memoryLimitBytes, const FeasibilityPolicy& policy) {
  if (!memoryLimitBytes) {
    return {FeasibilityStatus::Unknown, "no memory limit configured"};
  }
  const double limit = static_cast<double>(*memoryLimitBytes);
  if (static_cast<double>(estimate.staticStorageBytes) > limit || estimate.staticStorageSaturated) {
    return {FeasibilityStatus::HighRisk,
            "static storage alone (" + mb(static_cast<double>(estimate.staticStorageBytes)) + ") exceeds the limit"};
  }
  if (estimate.estimatedMinimumBytes && *estimate.estimatedMinimumBytes > limit) {
    return {FeasibilityStatus::HighRisk,
            "estimated minimum footprint (" + mb(*estimate.estimatedMinimumBytes) + ") exceeds the limit"};
  }
  if (knownUpperBoundBytes) {
    if (*knownUpperBoundBytes <= policy.memorySafeFraction * limit) {
      return {FeasibilityStatus::LikelySafe, "known peak memory is well within the limit"};
    }
    if (*knownUpperBoundBytes <= limit) {
      return {FeasibilityStatus::Borderline, "known peak memory is close to the limit"};
    }
    return {FeasibilityStatus::HighRisk, "known peak memory exceeds the limit"};
  }
  return {FeasibilityStatus::Unknown,
          "only lower bounds are known; peak memory requires measurement or a complete allocation model"};
}

}  // namespace bigolens::feasibility
