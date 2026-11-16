# Cross-component tests.
#
#   integration/  C++ source -> Clang AST -> static analysis -> constraint
#                 evaluation -> JSON response (requires the Clang frontend)
#   engine CLI    the bigolens-engine executable driven through files, exactly
#                 as the VS Code extension uses it

file(TO_CMAKE_PATH "${PROJECT_SOURCE_DIR}/shared/examples" _bigolens_examples_dir)
set(BIGOLENS_EXAMPLES_DIR "${_bigolens_examples_dir}")

if(BIGOLENS_ENABLE_CLANG)
  file(TO_CMAKE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/cpp" _bigolens_fixture_dir)
  add_executable(bigolens_integration_tests integration/frontend_pipeline_test.cpp integration/loop_forms_test.cpp
                                            integration/resource_dir_test.cpp)
  target_link_libraries(bigolens_integration_tests PRIVATE bigolens_clang_frontend bigolens_core GTest::gtest_main)
  target_compile_definitions(bigolens_integration_tests
                             PRIVATE BIGOLENS_FIXTURE_DIR="${_bigolens_fixture_dir}")
  bigolens_set_warnings(bigolens_integration_tests)
  gtest_discover_tests(bigolens_integration_tests DISCOVERY_MODE PRE_TEST PROPERTIES LABELS "integration"
                       DISCOVERY_TIMEOUT 60)
endif()

function(bigolens_engine_cli_test name request pass_regex)
  add_test(NAME engine_cli.${name} COMMAND bigolens-engine --request "${BIGOLENS_EXAMPLES_DIR}/${request}")
  set_tests_properties(engine_cli.${name} PROPERTIES PASS_REGULAR_EXPRESSION "${pass_regex}" LABELS "integration"
                                                     TIMEOUT 60)
endfunction()

bigolens_engine_cli_test(capabilities capabilities.request.json "\"status\":\"ok\"")
bigolens_engine_cli_test(benchmark_not_implemented benchmark.request.json "\"code\":\"not_implemented\"")
bigolens_engine_cli_test(invalid_request invalid.request.json "\"code\":\"invalid_request\"")
bigolens_engine_cli_test(malformed_json malformed.request.txt "\"code\":\"malformed_json\"")
if(BIGOLENS_ENABLE_CLANG)
  bigolens_engine_cli_test(analyze analyze.request.json "\"method\":\"sum-constraint-packing\"")
else()
  bigolens_engine_cli_test(analyze_without_frontend analyze.request.json "\"code\":\"frontend_unavailable\"")
endif()

add_test(NAME engine_cli.version COMMAND bigolens-engine --version)
set_tests_properties(engine_cli.version PROPERTIES PASS_REGULAR_EXPRESSION "bigolens-clang-engine [0-9]+\\.[0-9]+\\.[0-9]+"
                                                   LABELS "integration")
add_test(NAME engine_cli.unknown_argument COMMAND bigolens-engine --definitely-not-an-option)
set_tests_properties(engine_cli.unknown_argument PROPERTIES WILL_FAIL TRUE LABELS "integration")
