# Third-party dependencies.
#
# Each dependency is first searched for on the system (find_package). If it is
# not installed, a pinned release archive is downloaded and verified by SHA-256.
include(FetchContent)

FetchContent_Declare(
  nlohmann_json
  URL https://github.com/nlohmann/json/releases/download/v3.11.3/json.tar.xz
  URL_HASH SHA256=d6c65aca6b1ed68e7a182f4757257b107ae403032760ed6ef121c9d55e81757d
  DOWNLOAD_EXTRACT_TIMESTAMP TRUE
  FIND_PACKAGE_ARGS 3.11 CONFIG)
FetchContent_MakeAvailable(nlohmann_json)

if(BIGOLENS_BUILD_TESTS)
  # Keep GoogleTest's runtime consistent with ours on Windows.
  set(gtest_force_shared_crt
      ON
      CACHE BOOL "" FORCE)
  set(INSTALL_GTEST
      OFF
      CACHE BOOL "" FORCE)
  FetchContent_Declare(
    googletest
    URL https://github.com/google/googletest/releases/download/v1.15.2/googletest-1.15.2.tar.gz
    URL_HASH SHA256=7b42b4d6ed48810c5362c265a17faebe90dc2373c885e5216439d37927f02926
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE
    FIND_PACKAGE_ARGS 1.14 NAMES GTest)
  FetchContent_MakeAvailable(googletest)
  include(GoogleTest)
endif()
