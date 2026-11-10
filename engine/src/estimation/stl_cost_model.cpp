#include "bigolens/estimation/stl_cost_model.hpp"

namespace bigolens::estimation {
namespace {

using complexity::constant;
using complexity::logarithm;
using complexity::multiply;
using complexity::variable;

std::vector<StlOperationCost> buildTable() {
  const auto one = constant(1.0);
  const auto n = variable("n");
  const auto logN = logarithm(variable("n"));
  const auto nLogN = multiply({variable("n"), logarithm(variable("n"))});
  return {
      {"std::vector", "operator[]", one, one, false, "[vector.access]: constant time"},
      {"std::vector", "push_back", one, n, true,
       "[vector.modifiers]: amortized constant; a single call may reallocate and copy n elements"},
      {"std::vector", "erase", n, n, false,
       "[vector.modifiers]: linear in the number of elements after the erased one"},
      {"std::map", "find", logN, logN, false, "[associative.reqmts]: logarithmic in size"},
      {"std::map", "operator[]", logN, logN, false, "[map.access]: logarithmic in size"},
      {"std::set", "insert", logN, logN, false, "[associative.reqmts]: logarithmic in size"},
      {"std::set", "find", logN, logN, false, "[associative.reqmts]: logarithmic in size"},
      {"std::unordered_map", "find", one, n, false,
       "[unord.req]: average constant, worst case linear (hash collisions)"},
      {"std::unordered_map", "operator[]", one, n, false,
       "[unord.req]: average constant, worst case linear (hash collisions)"},
      {"std::priority_queue", "push", logN, logN, false, "[priqueue.members]: push_heap, logarithmic comparisons"},
      {"std::priority_queue", "pop", logN, logN, false, "[priqueue.members]: pop_heap, logarithmic comparisons"},
      {"std::priority_queue", "top", one, one, false, "[priqueue.members]: constant"},
      {"algorithm", "sort", nLogN, nLogN, false, "[sort]: O(n log n) comparisons (C++11 and later)"},
      {"algorithm", "lower_bound", logN, logN, false,
       "[lower.bound]: log n comparisons; linear iterator steps for non-random-access iterators"},
  };
}

}  // namespace

const std::vector<StlOperationCost>& stlCostTable() {
  static const std::vector<StlOperationCost> table = buildTable();
  return table;
}

std::optional<StlOperationCost> lookupStlCost(std::string_view container, std::string_view operation) {
  for (const auto& entry : stlCostTable()) {
    if (entry.container == container && entry.operation == operation) {
      return entry;
    }
  }
  return std::nullopt;
}

}  // namespace bigolens::estimation
