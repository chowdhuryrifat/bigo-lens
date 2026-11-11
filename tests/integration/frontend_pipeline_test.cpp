// End-to-end tests: C++ fixture -> Clang AST -> static analysis -> constraint
// evaluation -> JSON response, through the same RequestHandler the CLI uses.
#include <gtest/gtest.h>

#include <fstream>
#include <memory>
#include <nlohmann/json.hpp>
#include <sstream>
#include <string>

#include "bigolens/api/request_handler.hpp"
#include "bigolens/parser/clang_frontend.hpp"

using nlohmann::json;

namespace {

std::string readFixture(const std::string& name) {
  std::ifstream in(std::string(BIGOLENS_FIXTURE_DIR) + "/" + name, std::ios::binary);
  if (!in) {
    ADD_FAILURE() << "missing fixture " << name;
    return {};
  }
  std::ostringstream text;
  text << in.rdbuf();
  return text.str();
}

class FrontendPipeline : public ::testing::Test {
 protected:
  static void SetUpTestSuite() { analyzer_ = bigolens::parser::makeClangSourceAnalyzer().release(); }
  static void TearDownTestSuite() {
    delete analyzer_;
    analyzer_ = nullptr;
  }

  static json analyze(const std::string& fixture, json constraints = json::object()) {
    json request = {{"schema_version", "1.0"},
                    {"request_id", "it-" + fixture.substr(0, 2)},
                    {"action", "analyze"},
                    {"source", {{"path", fixture}, {"language", "cpp"}, {"content", readFixture(fixture)}}},
                    {"constraints", std::move(constraints)}};
    const bigolens::api::RequestHandler handler(analyzer_);
    return handler.handle(request);
  }

  static json analyzeSource(const std::string& content, json constraints = json::object()) {
    json request = {{"schema_version", "1.0"},
                    {"request_id", "it-inline"},
                    {"action", "analyze"},
                    {"source", {{"path", "inline.cpp"}, {"language", "cpp"}, {"content", content}}},
                    {"constraints", std::move(constraints)}};
    const bigolens::api::RequestHandler handler(analyzer_);
    return handler.handle(request);
  }

  /// Findings of the given category.
  static std::vector<json> findings(const json& response, const std::string& category) {
    std::vector<json> out;
    for (const auto& f : response["results"]["findings"]) {
      if (f["category"] == category) {
        out.push_back(f);
      }
    }
    return out;
  }

  static json single(const json& response, const std::string& category) {
    const auto list = findings(response, category);
    EXPECT_EQ(list.size(), 1U) << response.dump(2);
    return list.empty() ? json::object() : list.front();
  }

  static json findingByFunction(const json& response, const std::string& functionName) {
    for (const auto& f : response["results"]["findings"]) {
      if (f.contains("function") && f["function"] == functionName) {
        return f;
      }
    }
    return json::object();
  }

  static json bounds(std::initializer_list<std::pair<const char*, std::int64_t>> maxima) {
    json variables = json::object();
    for (const auto& [name, max] : maxima) {
      variables[name] = {{"min", 1}, {"max", max}};
    }
    return {{"variables", variables}};
  }

  static bigolens::analyzers::SourceAnalyzer* analyzer_;
};

bigolens::analyzers::SourceAnalyzer* FrontendPipeline::analyzer_ = nullptr;

TEST_F(FrontendPipeline, LinearLoop) {
  const json r = analyze("01_linear_loop.cpp", bounds({{"n", 1000}}));
  ASSERT_EQ(r["status"], "ok") << r.dump(2);
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "linear-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["asymptotic"], "O(n)");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 1000.0);
  EXPECT_EQ(f["function"], "main");
  EXPECT_EQ(f["location"]["line"], 8);
}

TEST_F(FrontendPipeline, NestedLoops) {
  const json r = analyze("02_nested_loops.cpp", bounds({{"n", 100}, {"m", 300}}));
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "nested-loop");
  EXPECT_EQ(f["time"]["asymptotic"], "O(m n)");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 30000.0);
}

TEST_F(FrontendPipeline, TriangularLoopHasExactStructuralCount) {
  const json r = analyze("03_triangular_loop.cpp", bounds({{"n", 200000}}));
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "triangular-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["asymptotic"], "O(n^2)");
  // N(N-1)/2 for N = 200000.
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 200000.0 * 199999.0 / 2.0);
}

TEST_F(FrontendPipeline, LogarithmicLoop) {
  const json r = analyze("04_logarithmic_loop.cpp", bounds({{"n", 1024}}));
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "logarithmic-loop");
  EXPECT_EQ(f["time"]["asymptotic"], "O(log n)");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 11.0);  // log2(1024) + 1
}

TEST_F(FrontendPipeline, SortingReportsVectorAndLinearInputLoop) {
  const json r = analyze("05_sorting.cpp", bounds({{"n", 1000}}));
  ASSERT_EQ(r["status"], "ok");
  const auto loops = findings(r, "loop_nest");
  ASSERT_EQ(loops.size(), 2U);
  EXPECT_EQ(loops[0]["pattern"], "linear-loop");
  EXPECT_EQ(loops[0]["time"]["asymptotic"], "O(n)");
  EXPECT_EQ(loops[1]["pattern"], "stl-algorithm");
  EXPECT_EQ(loops[1]["time"]["asymptotic"], "O(n log n)");
  EXPECT_NEAR(loops[1]["time"]["workload"]["upper_bound"].get<double>(), 1000.0 * std::log2(1000.0), 1e-3);
  const json vec = single(r, "dynamic_allocation");
  EXPECT_EQ(vec["space"]["expression"], "4 * n");
  EXPECT_EQ(vec["bound_status"], "estimated");
}

TEST_F(FrontendPipeline, SetInsertInLoopReportsNLogN) {
  const json r = analyzeSource(
      "namespace std {\n"
      "template <typename T> class set { public: void insert(const T&); };\n"
      "}\n"
      "void run(int n) {\n"
      "  std::set<int> s;\n"
      "  for (int i = 0; i < n; ++i) {\n"
      "    s.insert(i);\n"
      "  }\n"
      "}\n",
      bounds({{"n", 1000}}));
  ASSERT_EQ(r["status"], "ok");
  const json loop = single(r, "loop_nest");
  EXPECT_EQ(loop["pattern"], "linear-loop");
  EXPECT_EQ(loop["time"]["asymptotic"], "O(n log n)");
  EXPECT_EQ(loop["bound_status"], "proven");
  EXPECT_TRUE(loop["limitations"].empty());
}

TEST_F(FrontendPipeline, UnorderedMapStatesAverageVersusWorstAssumption) {
  const json r = analyzeSource(
      "namespace std {\n"
      "template <typename K, typename V> class unordered_map { public: void* find(const K&) const; };\n"
      "}\n"
      "void run(const std::unordered_map<int, int>& u, int n) {\n"
      "  for (int i = 0; i < n; ++i) {\n"
      "    u.find(i);\n"
      "  }\n"
      "}\n",
      bounds({{"n", 1000}}));
  ASSERT_EQ(r["status"], "ok");
  const json loop = single(r, "loop_nest");
  EXPECT_EQ(loop["bound_status"], "estimated");
  EXPECT_EQ(loop["confidence"], "low");
  EXPECT_EQ(loop["time"]["asymptotic"], "O(n)");
  bool foundAssumption = false;
  for (const auto& a : loop["assumptions"]) {
    if (a.get<std::string>().find("hash collisions") != std::string::npos ||
        a.get<std::string>().find("average-case") != std::string::npos) {
      foundAssumption = true;
      break;
    }
  }
  EXPECT_TRUE(foundAssumption);
  ASSERT_FALSE(loop["limitations"].empty());
  EXPECT_NE(loop["limitations"][0].get<std::string>().find("worst-case"), std::string::npos);
}

TEST_F(FrontendPipeline, BinarySearchWhileLoopIsUnknownNotGuessed) {
  const json r = analyze("06_binary_search.cpp", bounds({{"n", 1000}}));
  const auto loops = findings(r, "loop_nest");
  ASSERT_EQ(loops.size(), 2U);
  EXPECT_EQ(loops[1]["pattern"], "data-dependent-loop");
  EXPECT_EQ(loops[1]["bound_status"], "unknown");
  EXPECT_EQ(loops[1]["confidence"], "none");
  EXPECT_TRUE(loops[1]["time"].is_null());
}

TEST_F(FrontendPipeline, NaiveFibonacciRecursionIsUnsupported) {
  const json r = analyze("07_naive_fibonacci.cpp");
  const json f = single(r, "recursion");
  EXPECT_EQ(f["function"], "fib");
  EXPECT_EQ(f["pattern"], "multiple-direct-recursion");
  EXPECT_EQ(f["bound_status"], "unsupported");
  EXPECT_TRUE(f["time"].is_null());
  // BL-202: two calls with arguments n - 1 and n - 2 and constant non-recursive work.
  ASSERT_FALSE(f["limitations"].empty());
  EXPECT_EQ(f["limitations"][0], "extracted recurrence: T(n) = T(n - 1) + T(n - 2) + O(1)");
}

TEST_F(FrontendPipeline, MergeSortIsLinearithmicProven) {
  const json r = analyze("forms/R01_recursion.cpp");
  const json f = findingByFunction(r, "merge_sort");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "multiple-direct-recursion");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["confidence"], "high");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(n log n)");
  EXPECT_EQ(f["time"]["expression"], "n * (log_2(n) + 1)");
}

TEST_F(FrontendPipeline, RecursiveBinarySearchIsLogarithmicProven) {
  const json r = analyze("forms/R01_recursion.cpp");
  const json f = findingByFunction(r, "binary_search");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "multiple-direct-recursion");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["confidence"], "high");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(log n)");
  EXPECT_EQ(f["time"]["expression"], "log_2(n) + 1");
}

TEST_F(FrontendPipeline, RecursiveLinearSumIsLinearProven) {
  const json r = analyze("forms/R01_recursion.cpp");
  const json f = findingByFunction(r, "linear_sum");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "direct-recursion");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["confidence"], "high");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(n)");
  EXPECT_EQ(f["time"]["expression"], "n");
}

TEST_F(FrontendPipeline, VariableDivisorNearMissIsUnsupported) {
  const json r = analyze("forms/R01_recursion.cpp");
  const json f = findingByFunction(r, "near_miss_variable_divisor");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "direct-recursion");
  EXPECT_EQ(f["bound_status"], "unsupported");
  EXPECT_EQ(f["confidence"], "none");
  EXPECT_TRUE(f["time"].is_null());
}

TEST_F(FrontendPipeline, InterproceduralLoopCallingHelperIsProduct) {
  const json r = analyze("forms/I01_interprocedural.cpp", bounds({{"n", 100}, {"m", 50}}));
  const json f = findingByFunction(r, "caller_loop");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "nested-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["confidence"], "high");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(m n)");
  EXPECT_EQ(f["time"]["expression"], "n * m");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 5000.0);
}

TEST_F(FrontendPipeline, InterproceduralChainedHelpersPropagateCost) {
  const json r = analyze("forms/I01_interprocedural.cpp", bounds({{"n", 20}, {"k", 10}}));
  const json f = findingByFunction(r, "caller_chain");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "nested-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["confidence"], "high");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(k n)");
  EXPECT_EQ(f["time"]["expression"], "n * k");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 200.0);
}

TEST_F(FrontendPipeline, InterproceduralUnknownHelperLeavesCallerUnknown) {
  const json r = analyze("forms/I01_interprocedural.cpp");
  const json f = findingByFunction(r, "caller_unknown");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "data-dependent-loop");
  EXPECT_EQ(f["bound_status"], "unknown");
  EXPECT_EQ(f["confidence"], "none");
  EXPECT_TRUE(f["time"].is_null());
  ASSERT_FALSE(f["limitations"].empty());
  EXPECT_NE(f["limitations"][0].get<std::string>().find("loop is not in canonical counted form"), std::string::npos);
}

TEST_F(FrontendPipeline, InterproceduralRecursiveHelperFallsBackToEstimated) {
  const json r = analyze("forms/I01_interprocedural.cpp");
  const json f = findingByFunction(r, "caller_recursive");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "linear-loop");
  EXPECT_EQ(f["bound_status"], "estimated");
  EXPECT_EQ(f["confidence"], "low");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(n)");
  ASSERT_FALSE(f["limitations"].empty());
  EXPECT_NE(f["limitations"][0].get<std::string>().find("calls user function 'helper_recursive'"), std::string::npos);
}

TEST_F(FrontendPipeline, InterproceduralCallCycleFallsBackToEstimated) {
  const json r = analyze("forms/I01_interprocedural.cpp");
  const json f = findingByFunction(r, "caller_cycle");
  ASSERT_FALSE(f.empty());
  EXPECT_EQ(f["pattern"], "linear-loop");
  EXPECT_EQ(f["bound_status"], "estimated");
  EXPECT_EQ(f["confidence"], "low");
  ASSERT_TRUE(f["time"].is_object());
  EXPECT_EQ(f["time"]["asymptotic"], "O(n)");
  ASSERT_FALSE(f["limitations"].empty());
  EXPECT_NE(f["limitations"][0].get<std::string>().find("calls user function 'helper_cycle_a'"), std::string::npos);
  EXPECT_NE(f["limitations"][0].get<std::string>().find("call cycle detected"), std::string::npos);
}

TEST_F(FrontendPipeline, LinearSpaceVectorAndStaticArray) {
  const json r = analyze("08_linear_space_vector.cpp", bounds({{"n", 1000}}));
  const json vec = single(r, "dynamic_allocation");
  EXPECT_EQ(vec["space"]["asymptotic"], "O(n)");
  EXPECT_EQ(vec["space"]["workload"]["upper_bound"], 8.0 * 1001.0);
  const json arr = single(r, "static_storage");
  EXPECT_EQ(arr["space"]["workload"]["upper_bound"], 4000.0);
  EXPECT_EQ(r["results"]["memory"]["static_storage_bytes"], 4000);
}

TEST_F(FrontendPipeline, SumConstrainedTestCasesUseConstraintRelationship) {
  json constraints = bounds({{"t", 10}, {"n", 200000}});
  constraints["sum_constraints"] = json::array({{{"variable", "n"}, {"count_variable", "t"}, {"max_total", 200000}}});
  const json r = analyze("09_sum_constrained_test_cases.cpp", constraints);
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "test-case-loop");
  EXPECT_EQ(f["time"]["workload"]["method"], "sum-constraint-packing");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 4e10);
  EXPECT_EQ(r["results"]["summary"]["max_time_workload"], 4e10);
}

TEST_F(FrontendPipeline, ProductConstraintTightensNestedLoopsWorkload) {
  json constraints = bounds({{"n", 1000000}, {"m", 1000000}});
  constraints["product_constraints"] =
      json::array({{{"variables", json::array({"n", "m"})}, {"max_product", 1000000}}});
  const json r = analyze("11_product_constrained_grid.cpp", constraints);
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "nested-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 1e6);
  EXPECT_EQ(f["time"]["workload"]["method"], "product-constraint-bound");
  EXPECT_EQ(r["results"]["summary"]["max_time_workload"], 1e6);
  for (const auto& a : f["time"]["workload"]["assumptions"]) {
    EXPECT_EQ(a.get<std::string>().find("not used to tighten"), std::string::npos);
  }
}

TEST_F(FrontendPipeline, LinearConstraintTightensNestedLoopsWorkload) {
  json constraints = bounds({{"n", 1000000}, {"m", 1000000}});
  constraints["linear_constraints"] = json::array({{{"coefficients", {{"n", 1}, {"m", 1}}}, {"bound", 200000}}});
  const json r = analyze("12_linear_constrained_loops.cpp", constraints);
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "nested-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 1e10);
  EXPECT_EQ(f["time"]["workload"]["method"], "linear-constraint-bound");
  EXPECT_EQ(r["results"]["summary"]["max_time_workload"], 1e10);
  for (const auto& a : f["time"]["workload"]["assumptions"]) {
    EXPECT_EQ(a.get<std::string>().find("not used to tighten"), std::string::npos);
  }
}

TEST_F(FrontendPipeline, MultipleSumConstraintsSeparableWorkload) {
  json constraints = bounds({{"t", 10000}, {"n", 200000}, {"m", 200000}});
  constraints["sum_constraints"] = json::array({
      {{"variable", "n"}, {"count_variable", "t"}, {"max_total", 200000}},
      {{"variable", "m"}, {"count_variable", "t"}, {"max_total", 300000}},
  });
  const json r = analyze("13_multi_sum_constrained.cpp", constraints);
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["pattern"], "test-case-loop");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["status"], "proven");
  EXPECT_DOUBLE_EQ(f["time"]["workload"]["upper_bound"].get<double>(), 500000.0);
  EXPECT_EQ(f["time"]["workload"]["method"], "sum-constraint-packing");
  EXPECT_DOUBLE_EQ(r["results"]["summary"]["max_time_workload"].get<double>(), 500000.0);
}

TEST_F(FrontendPipeline, DataDependentTerminationIsUnknown) {
  const json r = analyze("10_data_dependent_termination.cpp");
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["bound_status"], "unknown");
  EXPECT_EQ(f["pattern"], "data-dependent-loop");
}

TEST_F(FrontendPipeline, SequentialInnerLoopsAreSummed) {
  const json r = analyze("sequential_loops.cpp", bounds({{"n", 10}, {"m", 1000}}));
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["time"]["asymptotic"], "O(m n + n^2)");
  EXPECT_EQ(f["time"]["workload"]["upper_bound"], 10.0 * (10.0 + 1000.0));
}

TEST_F(FrontendPipeline, MissingConstraintsLeaveWorkloadUnknown) {
  const json r = analyze("01_linear_loop.cpp");
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["bound_status"], "proven");
  EXPECT_EQ(f["time"]["workload"]["status"], "unknown");
  EXPECT_TRUE(f["time"]["workload"]["upper_bound"].is_null());
  EXPECT_EQ(f["time"]["workload"]["missing_bounds"], json::array({"n"}));
}

TEST_F(FrontendPipeline, ModifiedIndexIsUnsupported) {
  const json r = analyze("neg_modified_index.cpp", bounds({{"n", 10}}));
  const json f = single(r, "loop_nest");
  EXPECT_EQ(f["bound_status"], "unsupported");
  EXPECT_TRUE(f["time"].is_null());
}

TEST_F(FrontendPipeline, InvalidSourceReportsDiagnosticsAndNoFindings) {
  const json r = analyze("neg_invalid_syntax.cpp");
  EXPECT_EQ(r["status"], "error");
  EXPECT_EQ(r["errors"][0]["code"], "source_parse_error");
  ASSERT_FALSE(r["diagnostics"].empty());
  EXPECT_EQ(r["diagnostics"][0]["severity"], "error");
  EXPECT_EQ(r["diagnostics"][0]["location"]["file"], "neg_invalid_syntax.cpp");
  EXPECT_TRUE(r["results"].is_null());
}

TEST_F(FrontendPipeline, TwoPointerWhileLoopIsLinearProven) {
  const json r = analyze("forms/L14_two_pointer.cpp", bounds({{"n", 1000}}));
  ASSERT_EQ(r["status"], "ok") << r.dump(2);
  const auto loops = findings(r, "loop_nest");
  ASSERT_GE(loops.size(), 8U);

  // two_pointer_basic: O(n), Proven
  EXPECT_EQ(loops[0]["function"], "two_pointer_basic");
  EXPECT_EQ(loops[0]["pattern"], "linear-loop");
  EXPECT_EQ(loops[0]["bound_status"], "proven");
  EXPECT_EQ(loops[0]["confidence"], "high");
  EXPECT_EQ(loops[0]["time"]["asymptotic"], "O(n)");
  EXPECT_EQ(loops[0]["time"]["expression"], "n");
  EXPECT_DOUBLE_EQ(loops[0]["time"]["workload"]["upper_bound"].get<double>(), 1000.0);

  // near_miss_one_path_no_update: data-dependent termination -> Unknown
  EXPECT_EQ(loops[5]["function"], "near_miss_one_path_no_update");
  EXPECT_EQ(loops[5]["pattern"], "data-dependent-loop");
  EXPECT_EQ(loops[5]["bound_status"], "unknown");
  EXPECT_EQ(loops[5]["confidence"], "none");
  EXPECT_TRUE(loops[5]["time"].is_null());
}

TEST_F(FrontendPipeline, FrontendVersionIsReported) {
  const json r = analyze("01_linear_loop.cpp");
  ASSERT_TRUE(r["engine"]["frontend"].is_string());
  EXPECT_NE(r["engine"]["frontend"].get<std::string>().find("clang"), std::string::npos);
}

}  // namespace
