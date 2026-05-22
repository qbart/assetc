# fmt
include(FetchContent)

# Build options for the fmt subproject. Set before MakeAvailable so they
# apply when fmt's CMakeLists is pulled in via add_subdirectory.
set(FMT_TEST    OFF CACHE BOOL "" FORCE)
set(FMT_DOC     OFF CACHE BOOL "" FORCE)
set(FMT_INSTALL OFF CACHE BOOL "" FORCE)

FetchContent_Declare(
    fmtlib
    QUIET
    GIT_REPOSITORY https://github.com/fmtlib/fmt.git
    GIT_TAG 12.1.0
)

# Build fmt as a static library without forcing BUILD_SHARED_LIBS on the rest
# of the project. fmt keys its library type off BUILD_SHARED_LIBS.
set(FMT_PREV_BUILD_SHARED_LIBS "${BUILD_SHARED_LIBS}")
set(BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(fmtlib)
set(BUILD_SHARED_LIBS "${FMT_PREV_BUILD_SHARED_LIBS}" CACHE BOOL "" FORCE)
