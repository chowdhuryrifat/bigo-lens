#include "bigolens/api/protocol.hpp"

#include <charconv>
#include <cmath>
#include <set>
#include <string_view>

#include "bigolens/version.hpp"

namespace bigolens::api {

using nlohmann::json;

namespace {

class Validator {
 public:
  std::vector<ProtocolError> errors;

  void error(std::string path, std::string message) {
    errors.push_back({"invalid_request", std::move(message), std::move(path)});
  }

  /// Rejects members not listed in `allowed`.
  void onlyKnown(const json& object, const std::string& path, std::initializer_list<std::string_view> allowed) {
    for (const auto& [key, value] : object.items()) {
      bool known = false;
      for (const auto& name : allowed) {
        known = known || key == name;
      }
      if (!known) {
        error(path + "/" + key, "unknown field '" + key + "'");
      }
    }
  }

  const json* object(const json& parent, const std::string& key, const std::string& path, bool required) {
    const auto it = parent.find(key);
    if (it == parent.end()) {
      if (required) {
        error(path + "/" + key, "missing required field '" + key + "'");
      }
      return nullptr;
    }
    if (!it->is_object()) {
      error(path + "/" + key, "'" + key + "' must be an object");
      return nullptr;
    }
    return &*it;
  }

  std::optional<std::string> string(const json& parent, const std::string& key, const std::string& path, bool required,
                                    std::size_t maxLength) {
    const auto it = parent.find(key);
    if (it == parent.end()) {
      if (required) {
        error(path + "/" + key, "missing required field '" + key + "'");
      }
      return std::nullopt;
    }
    if (!it->is_string()) {
      error(path + "/" + key, "'" + key + "' must be a string");
      return std::nullopt;
    }
    const auto& value = it->get_ref<const std::string&>();
    if (value.size() > maxLength) {
      error(path + "/" + key, "'" + key + "' exceeds the maximum length of " + std::to_string(maxLength));
      return std::nullopt;
    }
    return value;
  }

  /// Accepts JSON integers, and floats with an exact integral value (e.g. 2e5).
  std::optional<std::int64_t> integer(const json& value, const std::string& path, std::int64_t min, std::int64_t max) {
    std::optional<std::int64_t> result;
    if (value.is_number_integer() && !value.is_number_unsigned()) {
      result = value.get<std::int64_t>();
    } else if (value.is_number_unsigned()) {
      const auto u = value.get<std::uint64_t>();
      if (u <= static_cast<std::uint64_t>(constraints::kMaxConstraintMagnitude)) {
        result = static_cast<std::int64_t>(u);
      }
    } else if (value.is_number_float()) {
      const double d = value.get<double>();
      if (std::isfinite(d) && d == std::floor(d) && std::fabs(d) <= 1e18) {
        result = static_cast<std::int64_t>(d);
      }
    } else {
      error(path, "must be an integer");
      return std::nullopt;
    }
    if (!result || *result < min || *result > max) {
      error(path, "must be an integer in [" + std::to_string(min) + ", " + std::to_string(max) + "]");
      return std::nullopt;
    }
    return result;
  }

  std::optional<std::int64_t> integerField(const json& parent, const std::string& key, const std::string& path,
                                           std::int64_t min, std::int64_t max) {
    const auto it = parent.find(key);
    if (it == parent.end()) {
      return std::nullopt;
    }
    return integer(*it, path + "/" + key, min, max);
  }
};

bool isIdentifierChar(char c, bool first) {
  const bool alpha = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_';
  return first ? alpha : alpha || (c >= '0' && c <= '9');
}

/// Variable names are C++ identifiers, optionally followed by ".size()".
bool isValidVariableName(std::string_view name) {
  constexpr std::string_view kSizeSuffix = ".size()";
  if (name.size() > kSizeSuffix.size() && name.substr(name.size() - kSizeSuffix.size()) == kSizeSuffix) {
    name.remove_suffix(kSizeSuffix.size());
  }
  if (name.empty() || name.size() > 64) {
    return false;
  }
  for (std::size_t i = 0; i < name.size(); ++i) {
    if (!isIdentifierChar(name[i], i == 0)) {
      return false;
    }
  }
  return true;
}

bool isValidRequestId(std::string_view id) {
  if (id.empty() || id.size() > 128) {
    return false;
  }
  for (const char c : id) {
    const bool ok = isIdentifierChar(c, false) || c == '-' || c == '.' || c == ':';
    if (!ok) {
      return false;
    }
  }
  return true;
}

constexpr std::int64_t kMag = constraints::kMaxConstraintMagnitude;

void parseVariable(Validator& v, const json& node, const std::string& path, const std::string& name,
                   constraints::ConstraintSet& out) {
  if (!isValidVariableName(name)) {
    v.error(path, "invalid variable name '" + name + "'");
    return;
  }
  if (!node.is_object()) {
    v.error(path, "variable bound must be an object");
    return;
  }
  v.onlyKnown(node, path, {"min", "max"});
  constraints::VariableBound bound;
  bound.min = v.integerField(node, "min", path, -kMag, kMag);
  bound.max = v.integerField(node, "max", path, -kMag, kMag);
  out.variables[name] = bound;
}

void parseConstraints(Validator& v, const json& node, constraints::ConstraintSet& out) {
  const std::string base = "/constraints";
  v.onlyKnown(node, base,
              {"variables", "sum_constraints", "product_constraints", "linear_constraints", "time_limit_ms",
               "memory_limit_mb"});
  if (const json* vars = v.object(node, "variables", base, false)) {
    for (const auto& [name, bound] : vars->items()) {
      parseVariable(v, bound, base + "/variables/" + name, name, out);
    }
  }
  auto array = [&](const char* key) -> const json* {
    const auto it = node.find(key);
    if (it == node.end()) {
      return nullptr;
    }
    if (!it->is_array() || it->size() > 64) {
      v.error(base + "/" + key, std::string("'") + key + "' must be an array of at most 64 items");
      return nullptr;
    }
    return &*it;
  };
  if (const json* sums = array("sum_constraints")) {
    for (std::size_t i = 0; i < sums->size(); ++i) {
      const std::string path = base + "/sum_constraints/" + std::to_string(i);
      const json& item = (*sums)[i];
      if (!item.is_object()) {
        v.error(path, "must be an object");
        continue;
      }
      v.onlyKnown(item, path, {"variable", "count_variable", "max_total"});
      auto variable = v.string(item, "variable", path, true, 80);
      auto count = v.string(item, "count_variable", path, true, 80);
      std::optional<std::int64_t> total;
      if (item.contains("max_total")) {
        total = v.integerField(item, "max_total", path, 0, kMag);
      } else {
        v.error(path + "/max_total", "missing required field 'max_total'");
      }
      if (variable && count && total) {
        out.sums.push_back({*variable, *count, *total});
      }
    }
  }
  if (const json* products = array("product_constraints")) {
    for (std::size_t i = 0; i < products->size(); ++i) {
      const std::string path = base + "/product_constraints/" + std::to_string(i);
      const json& item = (*products)[i];
      if (!item.is_object() || !item.contains("variables") || !item["variables"].is_array() ||
          !item.contains("max_product")) {
        v.error(path, "must be an object with 'variables' (array) and 'max_product'");
        continue;
      }
      v.onlyKnown(item, path, {"variables", "max_product"});
      constraints::ProductConstraint product;
      for (const auto& name : item["variables"]) {
        if (!name.is_string()) {
          v.error(path + "/variables", "variable names must be strings");
          continue;
        }
        product.variables.push_back(name.get<std::string>());
      }
      if (auto bound = v.integerField(item, "max_product", path, 0, kMag)) {
        product.maxProduct = *bound;
        out.products.push_back(std::move(product));
      }
    }
  }
  if (const json* linear = array("linear_constraints")) {
    for (std::size_t i = 0; i < linear->size(); ++i) {
      const std::string path = base + "/linear_constraints/" + std::to_string(i);
      const json& item = (*linear)[i];
      if (!item.is_object() || !item.contains("coefficients") || !item["coefficients"].is_object() ||
          !item.contains("bound")) {
        v.error(path, "must be an object with 'coefficients' (object) and 'bound'");
        continue;
      }
      v.onlyKnown(item, path, {"coefficients", "bound"});
      constraints::LinearConstraint constraint;
      for (const auto& [name, coefficient] : item["coefficients"].items()) {
        if (auto c = v.integer(coefficient, path + "/coefficients/" + name, -kMag, kMag)) {
          constraint.coefficients[name] = *c;
        }
      }
      if (auto bound = v.integerField(item, "bound", path, -kMag, kMag)) {
        constraint.bound = *bound;
        out.linear.push_back(std::move(constraint));
      }
    }
  }
  out.timeLimitMs = v.integerField(node, "time_limit_ms", base, 1, 3'600'000);
  if (auto mb = v.integerField(node, "memory_limit_mb", base, 1, 1'048'576)) {
    out.memoryLimitBytes = *mb * 1024 * 1024;
  }
}

}  // namespace

ParseOutcome parseRequest(const json& document) {
  ParseOutcome outcome;
  Validator v;
  if (!document.is_object()) {
    v.error("", "request must be a JSON object");
    outcome.errors = std::move(v.errors);
    return outcome;
  }

  // Recover the request id first so error responses can be correlated.
  if (auto id = v.string(document, "request_id", "", true, 128)) {
    if (isValidRequestId(*id)) {
      outcome.requestId = *id;
    } else {
      v.error("/request_id", "request_id must be 1-128 characters from [A-Za-z0-9_.:-]");
    }
  }

  v.onlyKnown(document, "", {"schema_version", "request_id", "action", "source", "constraints", "options"});

  Request request;
  if (auto version = v.string(document, "schema_version", "", true, 16)) {
    request.schemaVersion = *version;
    const auto dot = version->find('.');
    int major = -1;
    int minor = -1;
    if (dot != std::string::npos && dot > 0 && dot + 1 < version->size() &&
        version->find_first_not_of("0123456789.") == std::string::npos &&
        version->find('.', dot + 1) == std::string::npos) {
      // from_chars never throws; out-of-range components stay -1 and are rejected.
      const char* begin = version->data();
      const char* end = begin + version->size();
      int parsedMajor = -1;
      int parsedMinor = -1;
      const auto majorResult = std::from_chars(begin, begin + dot, parsedMajor);
      const auto minorResult = std::from_chars(begin + dot + 1, end, parsedMinor);
      if (majorResult.ec == std::errc() && majorResult.ptr == begin + dot && minorResult.ec == std::errc() &&
          minorResult.ptr == end) {
        major = parsedMajor;
        minor = parsedMinor;
      }
    }
    if (major < 0) {
      v.error("/schema_version", "schema_version must have the form MAJOR.MINOR");
    } else if (major != kProtocolMajor || minor > kProtocolMinor) {
      outcome.errors.push_back({"unsupported_schema_version",
                                "engine supports schema version " + std::string(kProtocolVersion) +
                                    " (same major, minor <= " + std::to_string(kProtocolMinor) + ")",
                                "/schema_version"});
    }
  }

  if (auto action = v.string(document, "action", "", true, 32)) {
    if (*action == "capabilities") {
      request.action = Action::Capabilities;
    } else if (*action == "analyze") {
      request.action = Action::Analyze;
    } else if (*action == "benchmark") {
      request.action = Action::Benchmark;
    } else {
      v.error("/action", "unknown action '" + *action + "'");
    }
  }

  const bool needsSource = request.action == Action::Analyze || request.action == Action::Benchmark;
  if (const json* source = v.object(document, "source", "", needsSource)) {
    v.onlyKnown(*source, "/source", {"path", "language", "content"});
    if (auto path = v.string(*source, "path", "/source", true, 4096)) {
      request.sourcePath = *path;
    }
    if (auto language = v.string(*source, "language", "/source", true, 16); language && *language != "cpp") {
      v.error("/source/language", "only 'cpp' is supported");
    }
    if (auto content = v.string(*source, "content", "/source", true, kMaxSourceBytes)) {
      request.sourceContent = *content;
    }
  }

  if (const json* constraintsNode = v.object(document, "constraints", "", false)) {
    parseConstraints(v, *constraintsNode, request.constraints);
    for (auto& problem : constraints::validate(request.constraints)) {
      v.error("/constraints", std::move(problem));
    }
  }

  if (const json* options = v.object(document, "options", "", false)) {
    v.onlyKnown(*options, "/options", {"cpp_standard"});
    if (auto standard = v.string(*options, "cpp_standard", "/options", false, 16)) {
      static const std::set<std::string> kAllowed = {"c++11", "c++14", "c++17", "c++20", "c++23"};
      if (kAllowed.count(*standard) == 0) {
        v.error("/options/cpp_standard", "cpp_standard must be one of c++11, c++14, c++17, c++20, c++23");
      } else {
        request.cppStandard = *standard;
      }
    }
  }

  outcome.errors.insert(outcome.errors.end(), v.errors.begin(), v.errors.end());
  if (outcome.errors.empty()) {
    request.requestId = outcome.requestId.value_or("");
    outcome.request = std::move(request);
  }
  return outcome;
}

}  // namespace bigolens::api
