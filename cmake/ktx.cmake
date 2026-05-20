# ktx
include(FetchContent)

# Build options for the ktx subproject. Set before MakeAvailable so they
# apply when KTX-Software's CMakeLists is pulled in via add_subdirectory.
#
# NOTE: KTX_FEATURE_KTX1 must stay ON. In v4.4.2 the always-compiled KTX2
# reader (texture2.c) and writers reference ktxTexture1_* symbols that only
# exist when KTX1 sources are built, so KTX1=OFF fails to link.
set(KTX_FEATURE_TESTS     OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_TOOLS     OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_VK_UPLOAD ON  CACHE BOOL "" FORCE)
set(KTX_FEATURE_GL_UPLOAD OFF CACHE BOOL "" FORCE)
set(KTX_FEATURE_KTX1      ON  CACHE BOOL "" FORCE)
set(KTX_FEATURE_KTX2      ON  CACHE BOOL "" FORCE)
set(BASISU_SUPPORT_SSE    ON  CACHE BOOL "" FORCE)

FetchContent_Declare(
    ktx
    GIT_REPOSITORY    https://github.com/KhronosGroup/KTX-Software.git
    GIT_TAG           v4.4.2
    GIT_SHALLOW       TRUE
)

# Build ktx as a static library without forcing BUILD_SHARED_LIBS on the rest
# of the project. KTX-Software 4.4.2 dropped KTX_FEATURE_STATIC_LIBRARY and
# now keys the library type off BUILD_SHARED_LIBS.
set(KTX_PREV_BUILD_SHARED_LIBS "${BUILD_SHARED_LIBS}")
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(ktx)
set(BUILD_SHARED_LIBS "${KTX_PREV_BUILD_SHARED_LIBS}" CACHE BOOL "" FORCE)
