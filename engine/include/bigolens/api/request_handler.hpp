#pragma once

#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include <nlohmann/json.hpp>

#include "bigolens/analyzers/source_analyzer.hpp"
#include "bigolens/complexity/recurrence.hpp"

namespace bigolens::api {

/// Turns protocol requests into protocol responses. Never throws: every failure
/// becomes a structured error response.
class RequestHandler {
 public:
  /// `analyzer` may be null when the engine was built without a frontend.
  /// Uses standardRecurrenceSolvers() by default.
  explicit RequestHandler(analyzers::SourceAnalyzer* analyzer);
  /// `solvers` (non-owning, may be empty) resolve extracted recurrences, in order.
  RequestHandler(analyzers::SourceAnalyzer* analyzer, std::vector<const complexity::RecurrenceSolver*> solvers);

  RequestHandler(const RequestHandler&) = delete;
  RequestHandler& operator=(const RequestHandler&) = delete;
  RequestHandler(RequestHandler&& other) noexcept;
  RequestHandler& operator=(RequestHandler&& other) noexcept;
  ~RequestHandler() = default;

  [[nodiscard]] nlohmann::json handle(const nlohmann::json& request) const;
  /// Parses raw text first; malformed JSON yields a "malformed_json" error response.
  [[nodiscard]] nlohmann::json handleText(std::string_view requestText) const;

 private:
  /// May throw; handle() converts any exception into an internal_error response.
  [[nodiscard]] nlohmann::json handleParsed(const nlohmann::json& request) const;

  analyzers::SourceAnalyzer* analyzer_;
  std::vector<std::unique_ptr<complexity::RecurrenceSolver>> ownedSolvers_;
  std::vector<const complexity::RecurrenceSolver*> solvers_;
};

}  // namespace bigolens::api
