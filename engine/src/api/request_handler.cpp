#include "bigolens/api/request_handler.hpp"

#include <exception>
#include <iostream>
#include <optional>

#include "bigolens/api/protocol.hpp"
#include "bigolens/complexity/asymptotic.hpp"
#include "bigolens/constraints/workload.hpp"
#include "bigolens/estimation/memory_estimator.hpp"
#include "bigolens/estimation/stl_cost_model.hpp"
#include "bigolens/feasibility/feasibility.hpp"
#include "bigolens/version.hpp"

namespace bigolens::api {

using nlohmann::json;

namespace {

/// Maximum nesting depth accepted before invoking the recursive JSON parser.
constexpr int kMaxJsonDepth = 64;

bool depthWithinLimit(std::string_view text) {
  int depth = 0;
  bool inString = false;
  bool escaped = false;
  for (const char c : text) {
    if (inString) {
      if (escaped) {
        escaped = false;
      } else if (c == '\\') {
        escaped = true;
      } else if (c == '"') {
        inString = false;
      }
      continue;
    }
    if (c == '"') {
      inString = true;
    } else if (c == '{' || c == '[') {
      if (++depth > kMaxJsonDepth) {
        return false;
      }
    } else if (c == '}' || c == ']') {
      --depth;
    }
  }
  return true;
}

json engineInfo(const analyzers::SourceAnalyzer* analyzer) {
  return {{"name", kEngineName},
          {"version", kEngineVersion},
          {"protocol_version", kProtocolVersion},
          {"frontend", analyzer ? json(analyzer->frontendVersion()) : json(nullptr)}};
}

json makeResponse(const std::optional<std::string>& requestId, const analyzers::SourceAnalyzer* analyzer) {
  return {{"schema_version", kProtocolVersion},
          {"request_id", requestId ? json(*requestId) : json(nullptr)},
          {"status", "ok"},
          {"engine", engineInfo(analyzer)},
          {"results", nullptr},
          {"diagnostics", json::array()},
          {"errors", json::array()}};
}

json errorResponse(json response, const std::string& code, const std::string& message, const std::string& path = {}) {
  response["status"] = "error";
  json error = {{"code", code}, {"message", message}};
  if (!path.empty()) {
    error["path"] = path;
  }
  response["errors"].push_back(std::move(error));
  return response;
}

json optionalNumber(const std::optional<double>& value) {
  return value.has_value() ? json(value.value()) : json(nullptr);
}

json toJson(const models::SourceLocation& location) {
  json out = {{"file", location.file}, {"line", location.line}, {"column", location.column}};
  if (location.endLine) {
    out["end_line"] = *location.endLine;
  }
  if (location.endColumn) {
    out["end_column"] = *location.endColumn;
  }
  return out;
}

json toJson(const models::Diagnostic& diagnostic) {
  json out = {{"severity", models::toString(diagnostic.severity)}, {"message", diagnostic.message}};
  out["location"] = diagnostic.location ? toJson(*diagnostic.location) : json(nullptr);
  return out;
}

json toJson(const constraints::WorkloadResult& workload) {
  return {{"status", models::toString(workload.status)},
          {"upper_bound", workload.upperBound ? json(*workload.upperBound) : json(nullptr)},
          {"method", workload.method.empty() ? json(nullptr) : json(workload.method)},
          {"assumptions", workload.assumptions},
          {"missing_bounds", workload.missing}};
}

std::optional<std::string> asymptotic(const complexity::Expr& expr) {
  if (auto bound = complexity::asymptoticUpperBound(expr)) {
    return complexity::toString(*bound);
  }
  return std::nullopt;
}

json quantity(const complexity::ExprPtr& expr, const constraints::ConstraintSet& cs,
              std::optional<double>* workloadOut = nullptr) {
  if (!expr) {
    return nullptr;
  }
  const auto workload = constraints::evaluateWorkload(*expr, cs);
  if (workloadOut) {
    *workloadOut = workload.upperBound;
  }
  const auto big = asymptotic(*expr);
  return {{"expression", complexity::toString(*expr)},
          {"asymptotic", big ? json(*big) : json(nullptr)},
          {"variables", complexity::freeVariables(*expr)},
          {"workload", toJson(workload)}};
}

json capabilities() {
  json stl = json::array();
  for (const auto& entry : estimation::stlCostTable()) {
    stl.push_back({{"container", entry.container},
                   {"operation", entry.operation},
                   {"average", complexity::toString(*entry.average)},
                   {"worst", complexity::toString(*entry.worst)},
                   {"amortized", entry.amortized},
                   {"note", entry.note}});
  }
  return {{"actions", {"capabilities", "analyze", "benchmark"}},
          {"features",
           {{"loop_analysis", "partial"},
            {"while_countdown_loops", "partial"},
            {"range_for_loops", "partial"},
            {"recursion_analysis", "unsupported"},
            {"stl_cost_model", "data_only"},
            {"interprocedural_analysis", "unsupported"},
            {"constraint_workload", "partial"},
            {"static_memory_estimate", "partial"},
            {"time_feasibility", "requires_runtime_estimate"},
            {"benchmark", "not_implemented"}}},
          {"stl_cost_model", stl}};
}

}  // namespace

json RequestHandler::handleText(std::string_view requestText) const {
  if (requestText.size() > kMaxRequestBytes) {
    return errorResponse(makeResponse(std::nullopt, analyzer_), "request_too_large",
                         "request exceeds " + std::to_string(kMaxRequestBytes) + " bytes");
  }
  if (!depthWithinLimit(requestText)) {
    return errorResponse(makeResponse(std::nullopt, analyzer_), "malformed_json",
                         "JSON nesting deeper than " + std::to_string(kMaxJsonDepth) + " levels");
  }
  json document = json::parse(requestText, nullptr, /*allow_exceptions=*/false);
  if (document.is_discarded()) {
    return errorResponse(makeResponse(std::nullopt, analyzer_), "malformed_json", "request is not valid JSON");
  }
  return handle(document);
}

json RequestHandler::handle(const json& document) const {
  try {
    return handleParsed(document);
  } catch (const std::exception& ex) {
    std::cerr << "bigolens-engine: internal error: " << ex.what() << '\n';
    return errorResponse(makeResponse(std::nullopt, analyzer_), "internal_error",
                         std::string("internal engine error: ") + ex.what());
  } catch (...) {
    std::cerr << "bigolens-engine: internal error: unknown exception\n";
    return errorResponse(makeResponse(std::nullopt, analyzer_), "internal_error", "internal engine error");
  }
}

json RequestHandler::handleParsed(const json& document) const {
  ParseOutcome parsed = parseRequest(document);
  json response = makeResponse(parsed.requestId, analyzer_);
  if (!parsed.request) {
    response["status"] = "error";
    for (const auto& error : parsed.errors) {
      json item = {{"code", error.code}, {"message", error.message}};
      if (!error.path.empty()) {
        item["path"] = error.path;
      }
      response["errors"].push_back(std::move(item));
    }
    return response;
  }
  const Request& request = *parsed.request;

  try {
    switch (request.action) {
      case Action::Capabilities:
        response["results"] = capabilities();
        return response;
      case Action::Benchmark:
        return errorResponse(std::move(response), "not_implemented",
                             "dynamic benchmarking is planned for milestone M4 and is not implemented");
      case Action::Analyze:
        break;
    }

    if (analyzer_ == nullptr) {
      return errorResponse(std::move(response), "frontend_unavailable",
                           "this engine build has no C++ frontend (built with BIGOLENS_ENABLE_CLANG=OFF)");
    }

    analyzers::SourceInput input{request.sourcePath, request.sourceContent, request.cppStandard};
    const analyzers::SourceAnalysis analysis = analyzer_->analyze(input);
    for (const auto& diagnostic : analysis.diagnostics) {
      response["diagnostics"].push_back(toJson(diagnostic));
    }
    if (!analysis.parsed) {
      return errorResponse(std::move(response), "source_parse_error",
                           "the C++ source could not be parsed; see diagnostics");
    }

    const auto& cs = request.constraints;
    const constraints::ConstraintSet singleInstance = constraints::singleInstanceConstraints(cs);
    json findings = json::array();
    std::optional<double> maxWorkload;
    std::optional<std::string> dominant;
    for (const auto& finding : analysis.findings) {
      std::optional<double> workload;
      json item = {{"id", finding.id},
                   {"category", models::toString(finding.category)},
                   {"pattern", finding.pattern},
                   {"description", finding.description},
                   {"function", finding.functionName},
                   {"location", toJson(finding.location)},
                   {"time", quantity(finding.timeExpression, cs, &workload)},
                   {"space", quantity(finding.spaceExpression, singleInstance)},
                   {"bound_status", models::toString(finding.boundStatus)},
                   {"confidence", models::toString(finding.confidence)},
                   {"assumptions", finding.assumptions},
                   {"limitations", finding.limitations},
                   {"provenance", finding.provenance}};
      if (workload && (!maxWorkload || *workload > *maxWorkload)) {
        maxWorkload = workload;
        dominant = finding.id;
      }
      findings.push_back(std::move(item));
    }

    const auto memory = estimation::estimateMemory(analysis.storage, cs);
    json storage = json::array();
    for (const auto& item : analysis.storage) {
      storage.push_back(
          {{"name", item.name},
           {"kind", estimation::toString(item.kind)},
           {"type", item.typeName},
           {"exact_bytes", item.exactBytes ? json(*item.exactBytes) : json(nullptr)},
           {"symbolic_bytes", item.symbolicBytes ? json(complexity::toString(*item.symbolicBytes)) : json(nullptr)},
           {"location", toJson(item.location)}});
    }
    const auto timeAssessment = feasibility::assessTime(std::nullopt, cs.timeLimitMs);
    const auto memoryAssessment = feasibility::assessMemory(memory, std::nullopt, cs.memoryLimitBytes);

    response["results"] = {
        {"findings", std::move(findings)},
        {"memory",
         {{"static_storage_bytes", memory.staticStorageBytes},
          {"static_storage_saturated", memory.staticStorageSaturated},
          {"largest_local_array_bytes", memory.largestLocalArrayBytes},
          {"estimated_minimum_bytes", optionalNumber(memory.estimatedMinimumBytes)},
          {"estimated_minimum_status", models::toString(memory.estimatedMinimumStatus)},
          {"assumptions", memory.assumptions},
          {"items", std::move(storage)}}},
        {"feasibility",
         {{"time", {{"status", models::toString(timeAssessment.status)}, {"reason", timeAssessment.reason}}},
          {"memory", {{"status", models::toString(memoryAssessment.status)}, {"reason", memoryAssessment.reason}}}}},
        {"summary",
         {{"finding_count", analysis.findings.size()},
          {"max_time_workload", maxWorkload ? json(*maxWorkload) : json(nullptr)},
          {"dominant_finding_id", dominant ? json(*dominant) : json(nullptr)},
          {"coverage", "partial"}}}};
    return response;
  } catch (const std::exception& ex) {
    std::cerr << "bigolens-engine: internal error: " << ex.what() << '\n';
    json fresh = makeResponse(parsed.requestId, analyzer_);
    return errorResponse(std::move(fresh), "internal_error", std::string("internal engine error: ") + ex.what());
  }
}

}  // namespace bigolens::api
