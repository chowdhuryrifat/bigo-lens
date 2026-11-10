#include <gtest/gtest.h>

#include <nlohmann/json.hpp>
#include <string>

#include "bigolens/api/protocol.hpp"
#include "bigolens/api/request_handler.hpp"

using bigolens::api::RequestHandler;
using nlohmann::json;

namespace {

json base(const std::string& action) {
  return {{"schema_version", "1.0"}, {"request_id", "t-1"}, {"action", action}};
}

json withSource(json request) {
  request["source"] = {{"path", "a.cpp"}, {"language", "cpp"}, {"content", "int main() {}"}};
  return request;
}

std::string firstErrorCode(const json& response) {
  return response.at("errors").empty() ? std::string() : response["errors"][0]["code"].get<std::string>();
}

const RequestHandler kNoFrontend(nullptr);

}  // namespace

TEST(Protocol, CapabilitiesSucceedWithoutFrontend) {
  const json r = kNoFrontend.handle(base("capabilities"));
  EXPECT_EQ(r["status"], "ok");
  EXPECT_EQ(r["request_id"], "t-1");
  EXPECT_EQ(r["schema_version"], "1.0");
  EXPECT_EQ(r["results"]["features"]["benchmark"], "not_implemented");
  EXPECT_TRUE(r["engine"]["frontend"].is_null());
}

TEST(Protocol, MalformedJsonIsAStructuredError) {
  const json r = kNoFrontend.handleText("{\"schema_version\": ");
  EXPECT_EQ(r["status"], "error");
  EXPECT_EQ(firstErrorCode(r), "malformed_json");
  EXPECT_TRUE(r["request_id"].is_null());
}

TEST(Protocol, ExcessiveNestingIsRejectedBeforeParsing) {
  const std::string deep(1000, '[');
  EXPECT_EQ(firstErrorCode(kNoFrontend.handleText(deep)), "malformed_json");
}

TEST(Protocol, OversizedRequestIsRejected) {
  const std::string big(bigolens::api::kMaxRequestBytes + 1, ' ');
  EXPECT_EQ(firstErrorCode(kNoFrontend.handleText(big)), "request_too_large");
}

TEST(Protocol, UnknownFieldsAreRejected) {
  json request = base("capabilities");
  request["unexpected"] = 1;
  const json r = kNoFrontend.handle(request);
  EXPECT_EQ(r["status"], "error");
  EXPECT_EQ(firstErrorCode(r), "invalid_request");
  EXPECT_EQ(r["errors"][0]["path"], "/unexpected");
  EXPECT_EQ(r["request_id"], "t-1");
}

TEST(Protocol, IncompatibleSchemaVersionIsRejected) {
  json request = base("capabilities");
  request["schema_version"] = "2.0";
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "unsupported_schema_version");
  request["schema_version"] = "1.9";
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "unsupported_schema_version");
  request["schema_version"] = "9999999999999.0";  // Out of int range: must not throw.
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
  request["schema_version"] = "1.99999999999999";
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
  request["schema_version"] = "one";
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
}

TEST(Protocol, AnalyzeRequiresSource) {
  const json r = kNoFrontend.handle(base("analyze"));
  EXPECT_EQ(firstErrorCode(r), "invalid_request");
}

TEST(Protocol, AnalyzeWithoutFrontendReportsUnavailable) {
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(withSource(base("analyze")))), "frontend_unavailable");
}

TEST(Protocol, BenchmarkIsNotImplemented) {
  const json r = kNoFrontend.handle(withSource(base("benchmark")));
  EXPECT_EQ(r["status"], "error");
  EXPECT_EQ(firstErrorCode(r), "not_implemented");
  EXPECT_TRUE(r["results"].is_null());
}

TEST(Protocol, InvalidRequestIdIsRejected) {
  json request = base("capabilities");
  request["request_id"] = "bad id with spaces";
  const json r = kNoFrontend.handle(request);
  EXPECT_EQ(firstErrorCode(r), "invalid_request");
  EXPECT_TRUE(r["request_id"].is_null());
}

TEST(Protocol, ConstraintValuesAreRangeChecked) {
  json request = withSource(base("analyze"));
  request["constraints"] = {{"variables", {{"n", {{"max", 1e19}}}}}};
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
  request["constraints"] = {{"variables", {{"n", {{"max", 2.5}}}}}};
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
  request["constraints"] = {{"variables", {{"bad name", {{"max", 5}}}}}};
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
}

TEST(Protocol, IntegralFloatConstraintsAreAccepted) {
  json request = withSource(base("analyze"));
  request["constraints"] = {{"variables", {{"n", {{"min", 1}, {"max", 2e5}}}}}, {"time_limit_ms", 1000}};
  // Valid request; fails only because this handler has no frontend.
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "frontend_unavailable");
}

TEST(Protocol, SumConstraintWithoutBoundsIsRejected) {
  json request = withSource(base("analyze"));
  request["constraints"] = {
      {"sum_constraints", json::array({{{"variable", "n"}, {"count_variable", "t"}, {"max_total", 10}}})}};
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
}

TEST(Protocol, NonCppLanguageIsRejected) {
  json request = withSource(base("analyze"));
  request["source"]["language"] = "python";
  EXPECT_EQ(firstErrorCode(kNoFrontend.handle(request)), "invalid_request");
}
