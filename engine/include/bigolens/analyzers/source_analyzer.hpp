#pragma once

#include <string>
#include <vector>

#include "bigolens/estimation/memory_estimator.hpp"
#include "bigolens/models/finding.hpp"
#include "bigolens/models/types.hpp"

namespace bigolens::analyzers {

struct SourceInput {
  std::string displayPath;  ///< Path reported in locations.
  std::string content;      ///< Full C++ translation unit.
  std::string cppStandard = "c++20";
};

struct SourceAnalysis {
  bool parsed = false;  ///< False when the frontend reported errors.
  std::vector<models::Finding> findings;
  std::vector<estimation::StorageItem> storage;
  std::vector<models::Diagnostic> diagnostics;
};

/// Front-end independent interface for static analysis. Implementations must
/// not leak front-end (Clang) types through this interface.
class SourceAnalyzer {
 public:
  SourceAnalyzer() = default;
  SourceAnalyzer(const SourceAnalyzer&) = delete;
  SourceAnalyzer& operator=(const SourceAnalyzer&) = delete;
  SourceAnalyzer(SourceAnalyzer&&) = delete;
  SourceAnalyzer& operator=(SourceAnalyzer&&) = delete;
  virtual ~SourceAnalyzer() = default;

  [[nodiscard]] virtual SourceAnalysis analyze(const SourceInput& input) = 0;
  /// Version string of the underlying frontend, e.g. "clang 20.1.8".
  [[nodiscard]] virtual std::string frontendVersion() const = 0;
};

}  // namespace bigolens::analyzers
