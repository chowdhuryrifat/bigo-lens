#pragma once

#include <memory>
#include <string>

#include "bigolens/analyzers/source_analyzer.hpp"

namespace bigolens::parser {

struct ClangFrontendOptions {
  /// Clang resource directory (builtin headers such as stddef.h). When set it
  /// is used as is; otherwise see resolveResourceDir().
  std::string resourceDir;
  /// Resource directory shipped next to the engine executable (packaged
  /// extension). Used only if it exists and contains include/stddef.h.
  std::string bundledResourceDir;
};

/// Resource directory the frontend will use: `resourceDir` if set, else the
/// BIGOLENS_CLANG_RESOURCE_DIR environment variable, else a valid
/// `bundledResourceDir`, else the directory detected at build time (may be empty).
[[nodiscard]] std::string resolveResourceDir(const ClangFrontendOptions& options);

/// Creates the Clang LibTooling based analyzer. Clang types stay internal.
[[nodiscard]] std::unique_ptr<analyzers::SourceAnalyzer> makeClangSourceAnalyzer(ClangFrontendOptions options = {});

}  // namespace bigolens::parser
