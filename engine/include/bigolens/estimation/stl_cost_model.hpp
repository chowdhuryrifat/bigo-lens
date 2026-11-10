#pragma once

#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "bigolens/complexity/expression.hpp"

namespace bigolens::estimation {

/// Documented cost of one standard-library operation as a function of the
/// container size `n` (or range length for algorithms).
struct StlOperationCost {
  std::string container;  ///< e.g. "std::vector", or "algorithm" for <algorithm>.
  std::string operation;  ///< e.g. "push_back", "sort".
  complexity::ExprPtr average;
  complexity::ExprPtr worst;
  bool amortized = false;  ///< Average cost is amortized over a sequence of operations.
  std::string note;        ///< Standard reference / caveats.
};

/// Looks up the cost model for `container`/`operation`.
[[nodiscard]] std::optional<StlOperationCost> lookupStlCost(std::string_view container, std::string_view operation);

/// All modelled operations (for documentation and the capabilities action).
[[nodiscard]] const std::vector<StlOperationCost>& stlCostTable();

}  // namespace bigolens::estimation
