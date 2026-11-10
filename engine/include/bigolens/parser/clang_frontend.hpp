#pragma once

#include <memory>
#include <string>

#include "bigolens/analyzers/source_analyzer.hpp"

namespace bigolens::parser {

struct ClangFrontendOptions {
  /// Clang resource directory (builtin headers). Empty: use the build-time
  /// default, overridable with the BIGOLENS_CLANG_RESOURCE_DIR environment variable.
  std::string resourceDir;
};

/// Creates the Clang LibTooling based analyzer. Clang types stay internal.
[[nodiscard]] std::unique_ptr<analyzers::SourceAnalyzer> makeClangSourceAnalyzer(ClangFrontendOptions options = {});

}  // namespace bigolens::parser
