// bigolens-engine: reads one JSON request (stdin or --request FILE) and writes
// one JSON response to stdout. See docs/specifications/JSON_PROTOCOL.md.
#include <cstdio>
#include <exception>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <memory>
#include <string>
#include <string_view>

#include "bigolens/api/protocol.hpp"
#include "bigolens/api/request_handler.hpp"
#include "bigolens/version.hpp"

#ifdef BIGOLENS_HAS_CLANG
#include "bigolens/parser/clang_frontend.hpp"
#endif

namespace {

constexpr std::string_view kUsage =
    "Usage: bigolens-engine [--request FILE]\n"
    "       bigolens-engine --version | --help\n\n"
    "Reads one JSON request from FILE or standard input and writes one JSON\n"
    "response to standard output. Exit status is 0 whenever a response was\n"
    "written (including error responses), 2 for usage errors, 1 otherwise.\n";

/// Reads at most limit + 1 bytes so oversized input is detected without
/// buffering it entirely.
std::string readBounded(std::istream& in, std::size_t limit) {
  std::string data;
  data.reserve(std::size_t{64} * 1024);
  char buffer[64 * 1024];
  while (data.size() <= limit && in.read(buffer, sizeof buffer).gcount() > 0) {
    data.append(buffer, static_cast<std::size_t>(in.gcount()));
  }
  return data;
}

#ifdef BIGOLENS_HAS_CLANG
/// Directory of the running executable: /proc/self/exe on Linux, otherwise
/// argv[0] (the extension always starts the engine with an absolute path).
std::filesystem::path executableDirectory(const char* argv0) {
  std::error_code error;
  std::filesystem::path self = std::filesystem::read_symlink("/proc/self/exe", error);
  if (error || self.empty()) {
    self = std::filesystem::absolute(argv0 != nullptr ? argv0 : "", error);
  }
  return error ? std::filesystem::path() : self.parent_path();
}

bigolens::parser::ClangFrontendOptions frontendOptions(const char* argv0) {
  bigolens::parser::ClangFrontendOptions options;
  const std::filesystem::path dir = executableDirectory(argv0);
  if (!dir.empty()) {
    options.bundledResourceDir = (dir / "clang-resource").string();
  }
  return options;
}
#endif

}  // namespace

int main(int argc, char** argv) {
  try {
    std::string requestFile;
    for (int i = 1; i < argc; ++i) {
      const std::string_view arg = argv[i];
      if (arg == "--version") {
        std::cout << bigolens::kEngineName << ' ' << bigolens::kEngineVersion << " (protocol "
                  << bigolens::kProtocolVersion << ")\n";
#ifdef BIGOLENS_HAS_CLANG
        const std::string resourceDir = bigolens::parser::resolveResourceDir(frontendOptions(argv[0]));
        std::cout << "clang resource directory: " << (resourceDir.empty() ? "(none)" : resourceDir) << '\n';
#else
        std::cout << "clang frontend: not built\n";
#endif
        return 0;
      }
      if (arg == "--help" || arg == "-h") {
        std::cout << kUsage;
        return 0;
      }
      if (arg == "--request" && i + 1 < argc) {
        requestFile = argv[++i];
        continue;
      }
      std::cerr << "bigolens-engine: unknown argument '" << arg << "'\n" << kUsage;
      return 2;
    }

    std::string text;
    if (requestFile.empty()) {
      text = readBounded(std::cin, bigolens::api::kMaxRequestBytes);
    } else {
      std::ifstream file(requestFile, std::ios::binary);
      if (!file) {
        std::cerr << "bigolens-engine: cannot open request file '" << requestFile << "'\n";
        return 1;
      }
      text = readBounded(file, bigolens::api::kMaxRequestBytes);
    }

#ifdef BIGOLENS_HAS_CLANG
    std::unique_ptr<bigolens::analyzers::SourceAnalyzer> analyzer =
        bigolens::parser::makeClangSourceAnalyzer(frontendOptions(argv[0]));
#else
    std::unique_ptr<bigolens::analyzers::SourceAnalyzer> analyzer;
#endif
    const bigolens::api::RequestHandler handler(analyzer.get());
    std::cout << handler.handleText(text).dump() << '\n';
    std::cout.flush();
    return std::cout.good() ? 0 : 1;
  } catch (const std::exception& ex) {
    std::cerr << "bigolens-engine: fatal: " << ex.what() << '\n';
    return 1;
  }
}
