# Applies the project warning policy to a target.
function(bigolens_set_warnings target)
  if(MSVC)
    target_compile_options(${target} PRIVATE /W4 /permissive- /utf-8)
    if(BIGOLENS_WARNINGS_AS_ERRORS)
      target_compile_options(${target} PRIVATE /WX)
    endif()
  else()
    target_compile_options(
      ${target}
      PRIVATE -Wall
              -Wextra
              -Wpedantic
              -Wshadow
              -Wconversion
              -Wsign-conversion
              -Wnon-virtual-dtor
              -Wold-style-cast
              -Woverloaded-virtual)
    # GCC's -Wnull-dereference reports false positives inside libstdc++ and LLVM
    # headers at -O2 (e.g. std::shared_ptr), so it is enabled for Clang only.
    if(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
      target_compile_options(${target} PRIVATE -Wnull-dereference)
    endif()
    if(BIGOLENS_WARNINGS_AS_ERRORS)
      target_compile_options(${target} PRIVATE -Werror)
    endif()
  endif()
endfunction()
