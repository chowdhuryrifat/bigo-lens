# Native analysis engine.
#
#   bigolens_core            protocol, models, symbolic expressions, constraints,
#                            estimation and feasibility (no Clang dependency)
#   bigolens_clang_frontend  Clang LibTooling static analyzer (optional)
#   bigolens-engine          command-line executable speaking the JSON protocol

configure_file(src/version.hpp.in "${CMAKE_CURRENT_BINARY_DIR}/generated/bigolens/version.hpp" @ONLY)

add_library(
  bigolens_core STATIC
  src/api/protocol.cpp
  src/api/request_handler.cpp
  src/complexity/asymptotic.cpp
  src/complexity/evaluator.cpp
  src/complexity/expression.cpp
  src/constraints/constraint_set.cpp
  src/constraints/workload.cpp
  src/estimation/memory_estimator.cpp
  src/estimation/stl_cost_model.cpp
  src/feasibility/feasibility.cpp
  src/models/types.cpp)
add_library(bigolens::core ALIAS bigolens_core)
target_include_directories(bigolens_core PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}/include"
                                                "${CMAKE_CURRENT_BINARY_DIR}/generated")
target_link_libraries(bigolens_core PUBLIC nlohmann_json::nlohmann_json)
target_compile_features(bigolens_core PUBLIC cxx_std_20)
bigolens_set_warnings(bigolens_core)

if(BIGOLENS_ENABLE_CLANG)
  add_library(bigolens_clang_frontend STATIC src/parser/clang_frontend.cpp)
  add_library(bigolens::clang_frontend ALIAS bigolens_clang_frontend)
  target_link_libraries(bigolens_clang_frontend PUBLIC bigolens_core PRIVATE ${BIGOLENS_CLANG_LIBS})
  target_include_directories(bigolens_clang_frontend SYSTEM PRIVATE ${LLVM_INCLUDE_DIRS} ${CLANG_INCLUDE_DIRS})
  target_compile_definitions(bigolens_clang_frontend PRIVATE ${LLVM_DEFINITIONS}
                             BIGOLENS_DEFAULT_CLANG_RESOURCE_DIR="${BIGOLENS_CLANG_RESOURCE_DIR}")
  # Subclassing Clang types requires matching LLVM's RTTI setting.
  if(NOT LLVM_ENABLE_RTTI AND NOT MSVC)
    target_compile_options(bigolens_clang_frontend PRIVATE -fno-rtti)
  endif()
  bigolens_set_warnings(bigolens_clang_frontend)
endif()

add_executable(bigolens-engine src/main.cpp)
target_link_libraries(bigolens-engine PRIVATE bigolens_core)
if(BIGOLENS_ENABLE_CLANG)
  target_link_libraries(bigolens-engine PRIVATE bigolens_clang_frontend)
  target_compile_definitions(bigolens-engine PRIVATE BIGOLENS_HAS_CLANG=1)
endif()
bigolens_set_warnings(bigolens-engine)

install(TARGETS bigolens-engine RUNTIME DESTINATION bin)

if(BIGOLENS_BUILD_TESTS)
  add_subdirectory(tests)
endif()
