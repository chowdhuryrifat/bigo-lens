#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

#include "bigolens/complexity/expression.hpp"
#include "bigolens/constraints/constraint_set.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::estimation {

enum class StorageKind { GlobalStatic, LocalArray, VectorBuffer };

/// A storage item discovered by static analysis.
struct StorageItem {
  std::string name;
  StorageKind kind = StorageKind::GlobalStatic;
  std::string typeName;
  /// Exact size for fixed-size objects (from the compiler's layout).
  std::optional<std::uint64_t> exactBytes;
  /// Symbolic size for dynamically sized buffers, in bytes.
  complexity::ExprPtr symbolicBytes;
  models::SourceLocation location;
};

struct MemoryEstimate {
  /// Sum of exact global/static storage. Proven lower bound on the footprint.
  std::uint64_t staticStorageBytes = 0;
  bool staticStorageSaturated = false;  ///< Sum overflowed and was clamped.
  /// Largest single local array (stack), if any.
  std::uint64_t largestLocalArrayBytes = 0;
  /// Lower bound including dynamically sized buffers evaluated at constraint maxima.
  std::optional<double> estimatedMinimumBytes;
  models::BoundStatus estimatedMinimumStatus = models::BoundStatus::Unknown;
  std::vector<std::string> assumptions;
};

/// Aggregates storage items. Never claims an upper bound on memory usage: heap
/// usage not visible to the analyzer and runtime overhead are not modelled.
[[nodiscard]] MemoryEstimate estimateMemory(const std::vector<StorageItem>& items,
                                            const constraints::ConstraintSet& constraints);

[[nodiscard]] const char* toString(StorageKind kind) noexcept;

}  // namespace bigolens::estimation
