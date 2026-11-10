cmake_minimum_required(VERSION 3.24)

project(
  BigOLens
  VERSION 0.1.0
  DESCRIPTION "Constraint-aware C++ complexity analyzer engine"
  LANGUAGES CXX)

option(BIGOLENS_ENABLE_CLANG "Build the Clang LibTooling frontend (static analysis)" ON)
option(BIGOLENS_BUILD_TESTS "Build unit and integration tests" ON)
option(BIGOLENS_WARNINGS_AS_ERRORS "Treat compiler warnings as errors" OFF)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(NOT CMAKE_BUILD_TYPE AND NOT CMAKE_CONFIGURATION_TYPES)
  set(CMAKE_BUILD_TYPE
      Debug
      CACHE STRING "Build type" FORCE)
endif()

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(CompilerWarnings)
include(Dependencies)

if(BIGOLENS_ENABLE_CLANG)
  include(ClangTooling)
endif()

if(BIGOLENS_BUILD_TESTS)
  enable_testing()
endif()

add_subdirectory(benchmark)
add_subdirectory(engine)

if(BIGOLENS_BUILD_TESTS)
  add_subdirectory(tests)
endif()

message(STATUS "BigO Lens ${PROJECT_VERSION}: Clang frontend = ${BIGOLENS_ENABLE_CLANG}, tests = ${BIGOLENS_BUILD_TESTS}")
