# ADR 0011: Ship a statically linked engine with Clang's builtin headers

- **Status:** Accepted
- **Date:** 2026-11-10
- **Related issue:** BL-108

## Context

The extension needs the native `bigolens-engine` at runtime. Development builds link Clang and LLVM dynamically
(`libclang-cpp.so`, `libLLVM.so`, together about 150 MB of shared libraries). They also embed the build machine's
Clang *resource directory*, which holds builtin headers such as `stddef.h`. A binary copied from such a build only
works on a machine with exactly the same LLVM packages installed at the same paths. That rules it out for a
`.vsix` that users install with one click.

Parsing a translation unit also needs the C++ standard library headers (`<iostream>`, `<vector>`). Those belong to
the user's toolchain, not to Clang.

## Decision

1. The packaged engine is linked **statically against the Clang and LLVM component libraries**. This is
   `BIGOLENS_STATIC_CLANG=ON` and the `package` CMake preset. Distribution packages often declare a dependency of
   their static Clang libraries on the monolithic shared `LLVM` library. `cmake/ClangTooling.cmake` replaces that
   dependency with an explicit list of static LLVM components.
2. **Clang's builtin headers are installed next to the engine**, in `bin/clang-resource/include`. At runtime the
   engine resolves the resource directory in this order:
   1. explicit option;
   2. the `BIGOLENS_CLANG_RESOURCE_DIR` environment variable;
   3. `<engine directory>/clang-resource`, if it contains `include/stddef.h`;
   4. the directory detected at build time.
3. The **C++ standard library headers stay a documented runtime dependency**. On Linux that means a GCC
   installation, for example `sudo apt install g++`. The engine does not bundle them.
4. `scripts/build/package-extension.sh` produces the `.vsix`. `scripts/test/verify-vsix.sh` checks three things:
   the package contents; the engine in a clean `ubuntu:24.04` container without LLVM; and the VS Code smoke suite
   running against the unpacked `.vsix`. Both run in CI (`package-linux`) and in the release workflow.

## Alternatives considered

- **Bundle the shared libraries.** `libclang-cpp` and `libLLVM` would ship inside the `.vsix` with an `rpath`. That
  is larger (the shared LLVM library alone exceeds 100 MB), and their own dependencies (`libffi`, `libedit`,
  `libxml2`, ICU) would leak into the package.
- **Document a runtime dependency on the distribution's LLVM packages.** This ties every user to one distribution
  and one LLVM version, and it breaks the one-click install.
- **Bundle a complete toolchain, including libstdc++ headers.** Analysis would then use headers that differ from
  the ones the user compiles with, and package size would grow further.

## Consequences

- **Positive:** the Linux `.vsix` works without LLVM installed. The engine needs only `libc`, `libstdc++`, `libm`,
  `libgcc_s`, `libz` and `libzstd`, all present on standard Ubuntu/Debian systems. The resource headers always
  match the linked Clang version.
- **Negative:**
  - The `.vsix` is about 18 MB. The stripped engine is about 48 MB and the headers about 15 MB, uncompressed.
  - Static linking takes longer than a development build.
  - The binary is tied to the glibc version of the build machine. The CI and release builds use Ubuntu 24.04, so
    older distributions are not supported.
- Development builds keep dynamic linking, which keeps edit-compile-test cycles fast.

## Follow-up considerations

- macOS and Windows packages (BL-603). On macOS the SDK's libc++ headers must match the Clang version (see
  TROUBLESHOOTING.md).
- Platform-specific `.vsix` targets (`vsce package --target linux-x64`) once more than one platform is packaged.
- Bundling the JavaScript (vsce warns about 869 files, most of them `ajv`) to reduce the package size and
  activation time.
