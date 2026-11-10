# Locates Clang LibTooling.
#
# Pass -DClang_DIR=<prefix>/lib/cmake/clang (and LLVM_DIR) when Clang is not
# discoverable automatically, e.g. -DClang_DIR=/usr/lib/llvm-18/lib/cmake/clang.
find_package(Clang REQUIRED CONFIG)

message(STATUS "Using LLVM ${LLVM_PACKAGE_VERSION} from ${LLVM_DIR}")
message(STATUS "Using ClangConfig.cmake in ${Clang_DIR}")

# Library selection.
#
# Default: the shared libraries shipped by distribution packages (libclang-cpp,
# libLLVM), which keeps development builds small and fast to link.
#
# BIGOLENS_STATIC_CLANG=ON links the individual static Clang/LLVM component
# libraries instead, producing an engine that does not need LLVM installed at
# runtime. This is what the packaged extension ships (ADR 0011).
option(BIGOLENS_STATIC_CLANG "Link Clang/LLVM statically (self-contained engine for packaging)" OFF)

set(_bigolens_clang_static_components
    clangTooling
    clangFrontend
    clangDriver
    clangParse
    clangSema
    clangSerialization
    clangAnalysis
    clangEdit
    clangAST
    clangLex
    clangBasic)
if(BIGOLENS_STATIC_CLANG)
  # LLVM's static targets reference zstd::libzstd_shared when LLVM was built
  # with zstd. Without the zstd development package the target is undefined;
  # define it from the runtime library (present on every Ubuntu/Debian system).
  if(LLVM_ENABLE_ZSTD AND NOT TARGET zstd::libzstd_shared)
    find_library(BIGOLENS_ZSTD_LIBRARY NAMES zstd libzstd.so.1 REQUIRED)
    add_library(zstd::libzstd_shared SHARED IMPORTED)
    set_target_properties(zstd::libzstd_shared PROPERTIES IMPORTED_LOCATION "${BIGOLENS_ZSTD_LIBRARY}")
    message(STATUS "Using zstd runtime library ${BIGOLENS_ZSTD_LIBRARY}")
  endif()
  foreach(_component IN LISTS _bigolens_clang_static_components)
    if(NOT TARGET ${_component})
      message(FATAL_ERROR "BIGOLENS_STATIC_CLANG=ON but the static Clang library '${_component}' is not available "
                          "(install the Clang development package that ships lib${_component}.a)")
    endif()
  endforeach()
  # Distribution builds of the static Clang libraries often declare a link
  # dependency on the monolithic shared "LLVM" library (LLVM_LINK_LLVM_DYLIB).
  # Replace it with the static LLVM components Clang's frontend needs, so no
  # LLVM shared library is required at runtime.
  llvm_map_components_to_libnames(
    BIGOLENS_LLVM_STATIC_LIBS
    support
    option
    frontendopenmp
    frontendhlsl
    frontenddriver
    targetparser
    binaryformat
    core
    mc
    mcparser
    profiledata
    bitreader
    bitstreamreader
    remarks
    demangle
    object
    textapi
    irreader
    asmparser
    windowsdriver)
  get_property(_bigolens_imported DIRECTORY "${CMAKE_SOURCE_DIR}" PROPERTY IMPORTED_TARGETS)
  foreach(_target IN LISTS _bigolens_imported)
    if(_target MATCHES "^clang")
      get_target_property(_links ${_target} INTERFACE_LINK_LIBRARIES)
      if(_links AND "LLVM" IN_LIST _links)
        list(TRANSFORM _links REPLACE "^LLVM$" "${BIGOLENS_LLVM_STATIC_LIBS}")
        set_target_properties(${_target} PROPERTIES INTERFACE_LINK_LIBRARIES "${_links}")
      endif()
    endif()
  endforeach()
  set(BIGOLENS_CLANG_LIBS ${_bigolens_clang_static_components} ${BIGOLENS_LLVM_STATIC_LIBS})
elseif(TARGET clang-cpp AND TARGET LLVM)
  set(BIGOLENS_CLANG_LIBS clang-cpp LLVM)
else()
  set(BIGOLENS_CLANG_LIBS ${_bigolens_clang_static_components})
  llvm_map_components_to_libnames(BIGOLENS_LLVM_COMPONENT_LIBS support option frontendopenmp targetparser)
  list(APPEND BIGOLENS_CLANG_LIBS ${BIGOLENS_LLVM_COMPONENT_LIBS})
endif()
message(STATUS "Clang libraries: ${BIGOLENS_CLANG_LIBS}")

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
