# Locates Clang LibTooling.
#
# Pass -DClang_DIR=<prefix>/lib/cmake/clang (and LLVM_DIR) when Clang is not
# discoverable automatically, e.g. -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang.
find_package(Clang REQUIRED CONFIG)

message(STATUS "Using LLVM ${LLVM_PACKAGE_VERSION} from ${LLVM_DIR}")
message(STATUS "Using ClangConfig.cmake in ${Clang_DIR}")

# Prefer the shared libraries shipped by distribution packages; fall back to
# the individual static components otherwise.
if(TARGET clang-cpp)
  set(BIGOLENS_CLANG_LIBS clang-cpp)
else()
  set(BIGOLENS_CLANG_LIBS clangTooling clangFrontend clangAST clangBasic clangLex clangSerialization)
endif()
if(TARGET LLVM)
  list(APPEND BIGOLENS_CLANG_LIBS LLVM)
else()
  llvm_map_components_to_libnames(BIGOLENS_LLVM_COMPONENT_LIBS support option frontendopenmp)
  list(APPEND BIGOLENS_CLANG_LIBS ${BIGOLENS_LLVM_COMPONENT_LIBS})
endif()

# Clang needs its resource directory (builtin headers such as stddef.h) when
# used as a library. Determine it at configure time from the matching clang
# binary; it can be overridden at runtime with BIGOLENS_CLANG_RESOURCE_DIR.
set(BIGOLENS_CLANG_RESOURCE_DIR
    ""
    CACHE PATH "Clang resource directory used by the frontend (auto-detected when empty)")
if(NOT BIGOLENS_CLANG_RESOURCE_DIR)
  find_program(
    BIGOLENS_CLANG_EXECUTABLE
    NAMES clang clang-${LLVM_VERSION_MAJOR}
    HINTS ${LLVM_TOOLS_BINARY_DIR}
    NO_DEFAULT_PATH)
  if(BIGOLENS_CLANG_EXECUTABLE)
    execute_process(
      COMMAND ${BIGOLENS_CLANG_EXECUTABLE} -print-resource-dir
      OUTPUT_VARIABLE _bigolens_resource_dir
      OUTPUT_STRIP_TRAILING_WHITESPACE)
    set(BIGOLENS_CLANG_RESOURCE_DIR "${_bigolens_resource_dir}")
  elseif(EXISTS "${LLVM_LIBRARY_DIR}/clang/${LLVM_VERSION_MAJOR}")
    set(BIGOLENS_CLANG_RESOURCE_DIR "${LLVM_LIBRARY_DIR}/clang/${LLVM_VERSION_MAJOR}")
  else()
    message(WARNING "Could not determine the Clang resource directory; set BIGOLENS_CLANG_RESOURCE_DIR")
  endif()
endif()
message(STATUS "Clang resource directory: ${BIGOLENS_CLANG_RESOURCE_DIR}")
