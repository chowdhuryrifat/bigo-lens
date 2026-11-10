#pragma once

#include <string>
#include <string_view>

#include <nlohmann/json.hpp>

#include "bigolens/analyzers/source_analyzer.hpp"

namespace bigolens::api {

/// Turns protocol requests into protocol responses. Never throws: every failure
/// becomes a structured error response.
class RequestHandler {
 public:
  /// `analyzer` may be null when the engine was built without a frontend.
  explicit RequestHandler(analyzers::SourceAnalyzer* analyzer) noexcept : analyzer_(analyzer) {}

  [[nodiscard]] nlohmann::json handle(const nlohmann::json& request) const;
  /// Parses raw text first; malformed JSON yields a "malformed_json" error response.
  [[nodiscard]] nlohmann::json handleText(std::string_view requestText) const;

 private:
  /// May throw; handle() converts any exception into an internal_error response.
  [[nodiscard]] nlohmann::json handleParsed(const nlohmann::json& request) const;

  analyzers::SourceAnalyzer* analyzer_;
};

}  // namespace bigolens::api
