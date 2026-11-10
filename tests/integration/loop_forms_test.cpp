// BL-201: one positive and at least one near-miss fixture per supported loop
// form (docs/specifications/COMPLEXITY_MODEL.md §2-3). Each row asserts the
// pattern, the symbolic time expression (or its absence), the bound status,
// the confidence and the leading limitation of one finding, in source order.
//
// The expected values were reviewed against the specification; a change here
// is a change in analyzer behaviour and must be justified in the PR.
#include <gtest/gtest.h>

#include <fstream>
#include <memory>
#include <nlohmann/json.hpp>
#include <sstream>
#include <string>
#include <vector>

#include "bigolens/api/request_handler.hpp"
#include "bigolens/parser/clang_frontend.hpp"

using nlohmann::json;

namespace {

struct ExpectedFinding {
  const char* function;
  const char* pattern;
  const char* boundStatus;
  const char* confidence;
  const char* timeExpression;    ///< nullptr: no time expression may be reported.
  const char* limitationPrefix;  ///< "" : no limitations expected.
};

struct FormCase {
  const char* fixture;
  std::vector<ExpectedFinding> findings;
};

const std::vector<FormCase>& formCases() {
  static const std::vector<FormCase> cases = {
      {"C01_nesting_and_dependence.cpp",
       {
           {"lower_triangle", "triangular-loop", "proven", "high", "sum_{i=0}^{n - 1} i", ""},
           {"upper_triangle", "triangular-loop", "proven", "high", "sum_{i=0}^{n - 1} (n - (i + 1))", ""},
           {"sequential_inner", "nested-loop", "proven", "high", "n * (n + m)", ""},
           {"nested_logarithmic", "nested-loop", "proven", "high", "n * (log_2(n) + 1)", ""},
           {"near_miss_strided_dependent", "data-dependent-loop", "unknown", "none", nullptr,
            "inner loop bounds depend on an index whose iteration space i"},
           {"near_miss_unknown_inner", "data-dependent-loop", "unknown", "none", nullptr, "data-dependent termination"},
       }},
      {"C02_status_rules.cpp",
       {
           {"calls_user_function", "linear-loop", "estimated", "low", "n", "calls user function 'helper'"},
           {"calls_library_function", "linear-loop", "proven", "medium", "n",
            "per-iteration cost of library calls is not included"},
           {"early_exit", "linear-loop", "proven", "medium", "n", ""},
           {"do_while", "unsupported-loop", "unsupported", "none", nullptr, "do-while loops are not modelled yet"},
           {"endless_for", "data-dependent-loop", "unknown", "none", nullptr,
            "loop is not in canonical counted form (init"},
           {"factorial", "direct-recursion", "unsupported", "none", nullptr,
            "time and stack depth of recursive functions are not estimate"},
       }},
      {"L01_increasing_unit_step.cpp",
       {
           {"lt_preincrement", "linear-loop", "proven", "high", "n", ""},
           {"lt_postincrement", "linear-loop", "proven", "high", "n", ""},
           {"assignment_init", "linear-loop", "proven", "high", "n", ""},
           {"not_equal_unit_step", "linear-loop", "proven", "high", "n", ""},
           {"nonzero_start", "linear-loop", "proven", "high", "n - 2", ""},
           {"reversed_comparison", "linear-loop", "proven", "high", "n", ""},
           {"near_miss_not_equal_stride", "data-dependent-loop", "unknown", "none", nullptr,
            "increasing index with a condition that does not bound it fro"},
           {"near_miss_index_written", "unsupported-loop", "unsupported", "none", nullptr,
            "loop index 'i' is modified in the body"},
           {"near_miss_bound_written", "linear-loop", "estimated", "low", "n",
            "bound variable 'n' may be modified inside the loop"},
           {"near_miss_floating_index", "unsupported-loop", "unsupported", "none", nullptr,
            "only integer loop indices are modelled"},
           {"near_miss_bound_from_array", "data-dependent-loop", "unknown", "none", nullptr,
            "loop start or bound is not a supported integer expression"},
       }},
      {"L02_increasing_inclusive.cpp",
       {
           {"le_from_one", "linear-loop", "proven", "high", "n", ""},
           {"near_miss_wrong_direction", "data-dependent-loop", "unknown", "none", nullptr,
            "increasing index with a condition that does not bound it fro"},
       }},
      {"L03_constant_stride.cpp",
       {
           {"stride_lt", "linear-loop", "proven", "high", "0.333333 * (n - 1) + 1", ""},
           {"stride_le", "linear-loop", "proven", "high", "0.25 * n + 1", ""},
           {"near_miss_variable_step", "data-dependent-loop", "unknown", "none", nullptr,
            "loop increment is not a constant step on the index"},
           {"near_miss_zero_step", "data-dependent-loop", "unknown", "none", nullptr,
            "loop increment is not a constant step on the index"},
       }},
      {"L04_decreasing_exclusive.cpp",
       {
           {"gt_predecrement", "linear-loop", "proven", "high", "n", ""},
           {"not_equal_decrement", "linear-loop", "proven", "high", "n", ""},
           {"near_miss_variable_decrement", "data-dependent-loop", "unknown", "none", nullptr,
            "loop increment is not a constant step on the index"},
           {"near_miss_wrong_direction", "data-dependent-loop", "unknown", "none", nullptr,
            "decreasing index with a condition that does not bound it fro"},
       }},
      {"L05_decreasing_inclusive.cpp",
       {
           {"ge_decrement", "linear-loop", "proven", "high", "n + 1", ""},
           {"ge_stride", "linear-loop", "proven", "high", "0.5 * (n - 1) + 1", ""},
           {"near_miss_unsigned_ge_zero", "unsupported-loop", "unsupported", "none", nullptr,
            "unsigned index compared with >= 0 never terminates"},
       }},
      {"L06_geometric_growth.cpp",
       {
           {"doubling", "logarithmic-loop", "proven", "high", "log_2(n) + 1", ""},
           {"tripling_inclusive", "logarithmic-loop", "proven", "high", "log_3(n) + 1", ""},
           {"shift_left", "logarithmic-loop", "proven", "high", "log_4(n) + 1", ""},
           {"near_miss_zero_start", "data-dependent-loop", "unknown", "none", nullptr,
            "geometric loop needs a constant start >= 1 and an upper boun"},
           {"near_miss_variable_start", "data-dependent-loop", "unknown", "none", nullptr,
            "geometric loop needs a constant start >= 1 and an upper boun"},
       }},
      {"L07_geometric_decay.cpp",
       {
           {"halving", "logarithmic-loop", "proven", "high", "log_2(n) + 1", ""},
           {"shift_right", "logarithmic-loop", "proven", "high", "log_2(n) + 1", ""},
           {"not_equal_zero", "logarithmic-loop", "proven", "high", "log_10(n) + 1", ""},
           {"near_miss_stop_above_one", "data-dependent-loop", "unknown", "none", nullptr,
            "halving loop must run while the index is positive"},
       }},
      {"L08_square_root.cpp",
       {
           {"is_prime", "sqrt-loop", "proven", "medium", "n^0.5 + 1", ""},
           {"count_below", "sqrt-loop", "proven", "high", "n^0.5 + 1", ""},
           {"near_miss_stride", "data-dependent-loop", "unknown", "none", nullptr,
            "square-root loop shape not supported"},
       }},
      {"L09_test_case_loop.cpp",
       {
           {"post_decrement", "test-case-loop", "proven", "high", "t * n", ""},
           {"post_decrement_compared", "test-case-loop", "proven", "high", "t", ""},
           {"near_miss_counter_written", "unsupported-loop", "unsupported", "none", nullptr,
            "counter 't' is modified in the body"},
           {"near_miss_pre_decrement", "data-dependent-loop", "unknown", "none", nullptr, "data-dependent termination"},
       }},
      {"L10_countdown_while.cpp",
       {
           {"countdown_gt", "linear-loop", "proven", "high", "x", ""},
           {"countdown_truthy", "linear-loop", "proven", "high", "x", ""},
           {"countdown_not_equal", "linear-loop", "proven", "high", "x", ""},
           {"countdown_ge_one", "linear-loop", "proven", "high", "x", ""},
           {"near_miss_two_updates", "data-dependent-loop", "unknown", "none", nullptr, "data-dependent termination"},
           {"near_miss_conditional_update", "data-dependent-loop", "unknown", "none", nullptr,
            "data-dependent termination"},
       }},
      {"L11_halving_while.cpp",
       {
           {"halve_divide", "logarithmic-loop", "proven", "high", "log_2(x) + 1", ""},
           {"halve_shift", "logarithmic-loop", "proven", "high", "log_2(x) + 1", ""},
           {"halve_unbraced", "logarithmic-loop", "proven", "high", "log_10(x) + 1", ""},
           {"near_miss_divide_by_one", "data-dependent-loop", "unknown", "none", nullptr, "data-dependent termination"},
           {"near_miss_assignment_form", "data-dependent-loop", "unknown", "none", nullptr,
            "data-dependent termination"},
       }},
      {"L12_range_for_container.cpp",
       {
           {"over_named_container", "range-for-loop", "proven", "high", "v.size()", ""},
           {"near_miss_temporary_range", "data-dependent-loop", "unknown", "none", nullptr,
            "range-based for over an expression that is not a named conta"},
           {"near_miss_element_as_bound", "data-dependent-loop", "unknown", "none", nullptr,
            "loop start or bound is not a supported integer expression"},
       }},
      {"L13_range_for_array.cpp",
       {
           {"over_local_array", "range-for-loop", "proven", "high", "16", ""},
           {"over_array_reference", "range-for-loop", "proven", "high", "8", ""},
       }},
  };
  return cases;
}

std::string readFixture(const std::string& name) {
  std::ifstream in(std::string(BIGOLENS_FIXTURE_DIR) + "/forms/" + name, std::ios::binary);
  std::ostringstream text;
  text << in.rdbuf();
  return text.str();
}

class LoopForms : public ::testing::TestWithParam<FormCase> {
 protected:
  static void SetUpTestSuite() { analyzer_ = bigolens::parser::makeClangSourceAnalyzer(); }
  static void TearDownTestSuite() { analyzer_.reset(); }
  static std::unique_ptr<bigolens::analyzers::SourceAnalyzer> analyzer_;
};

std::unique_ptr<bigolens::analyzers::SourceAnalyzer> LoopForms::analyzer_;

TEST_P(LoopForms, MatchesSpecification) {
  const FormCase& form = GetParam();
  const std::string source = readFixture(form.fixture);
  ASSERT_FALSE(source.empty()) << "missing fixture " << form.fixture;
  const json request = {{"schema_version", "1.0"},
                        {"request_id", "forms"},
                        {"action", "analyze"},
                        {"source", {{"path", form.fixture}, {"language", "cpp"}, {"content", source}}}};
  const json response = bigolens::api::RequestHandler(analyzer_.get()).handle(request);
  ASSERT_EQ(response["status"], "ok") << response.dump(2);

  std::vector<json> actual;
  for (const auto& finding : response["results"]["findings"]) {
    if (finding["category"] == "loop_nest" || finding["category"] == "recursion") {
      actual.push_back(finding);
    }
  }
  ASSERT_EQ(actual.size(), form.findings.size()) << response.dump(2);

  for (std::size_t i = 0; i < actual.size(); ++i) {
    const ExpectedFinding& expected = form.findings[i];
    const json& finding = actual[i];
    SCOPED_TRACE(std::string(form.fixture) + " :: " + expected.function);
    EXPECT_EQ(finding["function"].get<std::string>(), std::string(expected.function));
    EXPECT_EQ(finding["pattern"].get<std::string>(), std::string(expected.pattern));
    EXPECT_EQ(finding["bound_status"].get<std::string>(), std::string(expected.boundStatus));
    EXPECT_EQ(finding["confidence"].get<std::string>(), std::string(expected.confidence));
    if (expected.timeExpression == nullptr) {
      EXPECT_TRUE(finding["time"].is_null()) << finding["time"].dump();
    } else {
      ASSERT_TRUE(finding["time"].is_object());
      EXPECT_EQ(finding["time"]["expression"].get<std::string>(), std::string(expected.timeExpression));
    }
    const auto& limitations = finding["limitations"];
    if (std::string(expected.limitationPrefix).empty()) {
      EXPECT_TRUE(limitations.empty()) << limitations.dump();
    } else {
      ASSERT_FALSE(limitations.empty());
      EXPECT_EQ(limitations[0].get<std::string>().rfind(expected.limitationPrefix, 0), 0U)
          << limitations[0].get<std::string>();
    }
    // Invariant from COMPLEXITY_MODEL §3: no expression implies Unknown or Unsupported.
    if (finding["time"].is_null() && finding["category"] == "loop_nest") {
      EXPECT_TRUE(finding["bound_status"] == "unknown" || finding["bound_status"] == "unsupported");
      EXPECT_EQ(finding["confidence"], "none");
    }
  }
}

std::string caseName(const ::testing::TestParamInfo<FormCase>& info) {
  std::string name = info.param.fixture;
  name = name.substr(0, name.find('.'));
  for (char& c : name) {
    if (!((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9'))) {
      c = '_';
    }
  }
  return name;
}

INSTANTIATE_TEST_SUITE_P(Fixtures, LoopForms, ::testing::ValuesIn(formCases()), caseName);

}  // namespace
