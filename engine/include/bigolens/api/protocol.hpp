#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

#include "bigolens/constraints/constraint_set.hpp"

namespace bigolens::api {

/// Upper bound on the size of a request accepted on stdin or from a file.
inline constexpr std::size_t kMaxRequestBytes = std::size_t{4} * 1024 * 1024;
/// Upper bound on embedded source content.
inline constexpr std::size_t kMaxSourceBytes = std::size_t{2} * 1024 * 1024;

enum class Action { Capabilities, Analyze, Benchmark };

struct ProtocolError {
  std::string code;     ///< Stable machine-readable code, e.g. "invalid_request".
  std::string message;  ///< Human-readable explanation.
  std::string path;     ///< JSON pointer to the offending field, if applicable.
};

struct Request {
  std::string schemaVersion;
  std::string requestId;
  Action action = Action::Capabilities;
  std::string sourcePath;
  std::string sourceContent;
  constraints::ConstraintSet constraints;
  std::string cppStandard = "c++20";
};

/// Parsed request or the validation errors that prevented parsing.
struct ParseOutcome {
  std::optional<Request> request;
  std::optional<std::string> requestId;  ///< Recovered even if the request is invalid.
  std::vector<ProtocolError> errors;
};

/// Validates a JSON document against the request contract
/// (shared/schemas/request.schema.json) and converts it.
[[nodiscard]] ParseOutcome parseRequest(const nlohmann::json& document);

}  // namespace bigolens::api
