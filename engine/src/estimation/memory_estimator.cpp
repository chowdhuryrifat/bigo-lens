#include "bigolens/estimation/memory_estimator.hpp"

#include <algorithm>
#include <limits>

#include "bigolens/constraints/workload.hpp"

namespace bigolens::estimation {

const char* toString(StorageKind kind) noexcept {
  switch (kind) {
    case StorageKind::GlobalStatic:
      return "global_static";
    case StorageKind::LocalArray:
      return "local_array";
    case StorageKind::VectorBuffer:
      return "vector_buffer";
  }
  return "global_static";
}

MemoryEstimate estimateMemory(const std::vector<StorageItem>& items, const constraints::ConstraintSet& cs) {
  MemoryEstimate estimate;
  constexpr auto kMax = std::numeric_limits<std::uint64_t>::max();

  // Buffers are sized as a single live instance; summing per-test-case
  // allocations across test cases would overstate a lower bound.
  const constraints::ConstraintSet single = constraints::singleInstanceConstraints(cs);
  double dynamicBytes = 0.0;
  bool anyDynamic = false;
  bool dynamicKnown = true;
  for (const auto& item : items) {
    switch (item.kind) {
      case StorageKind::GlobalStatic:
        if (item.exactBytes) {
          if (estimate.staticStorageBytes > kMax - *item.exactBytes) {
            estimate.staticStorageBytes = kMax;
            estimate.staticStorageSaturated = true;
          } else {
            estimate.staticStorageBytes += *item.exactBytes;
          }
        }
        break;
      case StorageKind::LocalArray:
        if (item.exactBytes) {
          estimate.largestLocalArrayBytes = std::max(estimate.largestLocalArrayBytes, *item.exactBytes);
        }
        break;
      case StorageKind::VectorBuffer:
        if (item.symbolicBytes) {
          anyDynamic = true;
          const auto workload = constraints::evaluateWorkload(*item.symbolicBytes, single);
          if (workload.upperBound) {
            dynamicBytes += *workload.upperBound;
          } else {
            dynamicKnown = false;
            estimate.assumptions.push_back("size of '" + item.name + "' could not be evaluated under the constraints");
          }
        }
        break;
    }
  }

  if (anyDynamic && dynamicKnown) {
    estimate.estimatedMinimumBytes = static_cast<double>(estimate.staticStorageBytes) + dynamicBytes;
    estimate.estimatedMinimumStatus = models::BoundStatus::Estimated;
    estimate.assumptions.push_back(
        "sized std::vector buffers evaluated at constraint maxima, assuming each is constructed once with its "
        "largest single-instance size (sum constraints cap, but do not accumulate, per-case sizes); capacity slack and "
        "allocator overhead not included");
  } else if (!anyDynamic) {
    estimate.estimatedMinimumBytes = static_cast<double>(estimate.staticStorageBytes);
    estimate.estimatedMinimumStatus = models::BoundStatus::Proven;
  }
  estimate.assumptions.push_back(
      "process baseline, stack frames, standard-library internals and unmodelled heap allocations are not "
      "included; figures are lower bounds, not peak usage");
  return estimate;
}

}  // namespace bigolens::estimation
