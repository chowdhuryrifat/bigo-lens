# Benchmark subsystem (milestone M4).
#
# Currently contains only the measurement statistics and growth-model fitting
# library. It does not compile or execute user code; the compiler adapter,
# runner and resource monitor are specified in
# docs/specifications/BENCHMARK_SPECIFICATION.md and are not implemented yet.
add_library(bigolens_benchmark STATIC src/statistics.cpp src/growth_fit.cpp)
add_library(bigolens::benchmark ALIAS bigolens_benchmark)
target_include_directories(bigolens_benchmark PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/include")
target_compile_features(bigolens_benchmark PUBLIC cxx_std_20)
bigolens_set_warnings(bigolens_benchmark)

if(BIGOLENS_BUILD_TESTS)
  add_executable(bigolens_benchmark_tests tests/statistics_test.cpp tests/growth_fit_test.cpp)
  target_link_libraries(bigolens_benchmark_tests PRIVATE bigolens_benchmark GTest::gtest_main)
  bigolens_set_warnings(bigolens_benchmark_tests)
  gtest_discover_tests(bigolens_benchmark_tests DISCOVERY_MODE PRE_TEST PROPERTIES LABELS "unit")
endif()
