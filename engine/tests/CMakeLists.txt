add_executable(
  bigolens_engine_tests
  asymptotic_test.cpp
  constraints_test.cpp
  evaluator_test.cpp
  expression_test.cpp
  feasibility_test.cpp
  memory_estimator_test.cpp
  protocol_test.cpp
  stl_cost_model_test.cpp
  workload_test.cpp)
target_link_libraries(bigolens_engine_tests PRIVATE bigolens_core GTest::gtest_main)
bigolens_set_warnings(bigolens_engine_tests)
gtest_discover_tests(bigolens_engine_tests DISCOVERY_MODE PRE_TEST PROPERTIES LABELS "unit")
