// Resolution order of the Clang resource directory (ADR 0011):
// explicit option > BIGOLENS_CLANG_RESOURCE_DIR > valid bundled directory > build-time default.
#include <gtest/gtest.h>

#if defined(_WIN32)
#include <process.h>
inline int portable_setenv(const char* name, const char* value, int /*overwrite*/) {
  return _putenv_s(name, value);
}
inline int portable_unsetenv(const char* name) {
  return _putenv_s(name, "");
}
inline int portable_getpid() {
  return _getpid();
}
#else
#include <unistd.h>
inline int portable_setenv(const char* name, const char* value, int overwrite) {
  return ::setenv(name, value, overwrite);
}
inline int portable_unsetenv(const char* name) {
  return ::unsetenv(name);
}
inline int portable_getpid() {
  return ::getpid();
}
#endif

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <optional>
#include <string>

#include "bigolens/parser/clang_frontend.hpp"

namespace fs = std::filesystem;
using bigolens::parser::ClangFrontendOptions;
using bigolens::parser::resolveResourceDir;

namespace {

class ResourceDir : public ::testing::Test {
 protected:
  void SetUp() override {
    if (const char* env = std::getenv("BIGOLENS_CLANG_RESOURCE_DIR")) {
      savedEnv_ = env;
    }
    portable_unsetenv("BIGOLENS_CLANG_RESOURCE_DIR");
    root_ = fs::temp_directory_path() / ("bigolens-resource-test-" + std::to_string(portable_getpid()));
    fs::create_directories(root_ / "valid" / "include");
    std::ofstream(root_ / "valid" / "include" / "stddef.h") << "/* marker */\n";
    fs::create_directories(root_ / "empty");
  }
  void TearDown() override {
    fs::remove_all(root_);
    if (savedEnv_) {
      portable_setenv("BIGOLENS_CLANG_RESOURCE_DIR", savedEnv_->c_str(), 1);
    } else {
      portable_unsetenv("BIGOLENS_CLANG_RESOURCE_DIR");
    }
  }

  fs::path root_;
  std::optional<std::string> savedEnv_;
};

TEST_F(ResourceDir, ExplicitOptionWins) {
  portable_setenv("BIGOLENS_CLANG_RESOURCE_DIR", "/from/env", 1);
  ClangFrontendOptions options;
  options.resourceDir = "/explicit";
  options.bundledResourceDir = (root_ / "valid").string();
  EXPECT_EQ(resolveResourceDir(options), "/explicit");
}

TEST_F(ResourceDir, EnvironmentBeatsBundled) {
  portable_setenv("BIGOLENS_CLANG_RESOURCE_DIR", "/from/env", 1);
  ClangFrontendOptions options;
  options.bundledResourceDir = (root_ / "valid").string();
  EXPECT_EQ(resolveResourceDir(options), "/from/env");
}

TEST_F(ResourceDir, ValidBundledDirectoryIsUsed) {
  ClangFrontendOptions options;
  options.bundledResourceDir = (root_ / "valid").string();
  EXPECT_EQ(resolveResourceDir(options), (root_ / "valid").string());
}

TEST_F(ResourceDir, BundledDirectoryWithoutHeadersIsIgnored) {
  ClangFrontendOptions options;
  options.bundledResourceDir = (root_ / "empty").string();
  EXPECT_NE(resolveResourceDir(options), (root_ / "empty").string());
  options.bundledResourceDir = (root_ / "missing").string();
  EXPECT_NE(resolveResourceDir(options), (root_ / "missing").string());
}

}  // namespace
