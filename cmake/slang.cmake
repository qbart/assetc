include(FetchContent)

# Build options for the slang subproject. Set before MakeAvailable so they
# apply when slang's CMakeLists is pulled in via add_subdirectory.
set(SLANG_LIB_TYPE                  STATIC CACHE STRING "" FORCE)
set(SLANG_ENABLE_SLANG_GLSLANG      OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_TESTS              OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_EXAMPLES           OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_REPLAYER           OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_GFX                OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_SLANGD             OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_SLANGRT            OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_SLANGC             OFF    CACHE BOOL   "" FORCE)
set(SLANG_EMBED_CORE_MODULE         ON     CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_CUDA               OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_OPTIX              OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_NVAPI              OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_AFTERMATH          OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_SLANG_RHI          OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_SLANGI             OFF    CACHE BOOL   "" FORCE)
set(SLANG_ENABLE_PREBUILT_BINARIES  OFF    CACHE BOOL   "" FORCE)

# EXCLUDE_FROM_ALL so the default build target only builds what we actually
# link (the `slang` library + its embedded core module). Without it, `make`
# also builds optional/experimental targets like the `slang-neural-module`
# ALL target, which is compiled by a (possibly version-mismatched) system
# slangc and fails.
FetchContent_Declare(
    slang
    GIT_REPOSITORY    https://github.com/shader-slang/slang.git
    GIT_TAG           v2026.8.1
    GIT_SHALLOW       TRUE
    EXCLUDE_FROM_ALL  TRUE
)

FetchContent_MakeAvailable(slang)
