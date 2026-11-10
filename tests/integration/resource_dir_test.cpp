// Resolution order of the Clang resource directory (ADR 0011):
// explicit option > BIGOLENS_CLANG_RESOURCE_DIR > valid bundled directory > build-time default.
#include <gtest/gtest.h>

#include <unistd.h>
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
    ::unsetenv("BIGOLENS_CLANG_RESOURCE_DIR");
    root_ = fs::temp_directory_path() / ("bigolens-resource-test-" + std::to_string(::getpid()));
    fs::create_directories(root_ / "valid" / "include");
    std::ofstream(root_ / "valid" / "include" / "stddef.h") << "/* marker */\n";
    fs::create_directories(root_ / "empty");
  }
  void TearDown() override {
    fs::remove_all(root_);
    if (savedEnv_) {
      ::setenv("BIGOLENS_CLANG_RESOURCE_DIR", savedEnv_->c_str(), 1);
    } else {
      ::unsetenv("BIGOLENS_CLANG_RESOURCE_DIR");
    }
  }

  fs::path root_;
  std::optional<std::string> savedEnv_;
};

TEST_F(ResourceDir, ExplicitOptionWins) {
  ::setenv("BIGOLENS_CLANG_RESOURCE_DIR", "/from/env", 1);
  ClangFrontendOptions options;
  options.resourceDir = "/explicit";
  options.bundledResourceDir = (root_ / "valid").string();
  EXPECT_EQ(resolveResourceDir(options), "/explicit");
}

TEST_F(ResourceDir, EnvironmentBeatsBundled) {
  ::setenv("BIGOLENS_CLANG_RESOURCE_DIR", "/from/env", 1);
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
