# assetc SDK — standalone, dependency-free runtime loader library.
#
# This CMakeLists is self-contained so an engine can pull *only* this directory in
# via FetchContent without configuring the rest of the assetc repo (which fetches
# slang, ktx, meshoptimizer, etc.):
#
#   include(FetchContent)
#   FetchContent_Declare(assetc_sdk
#       GIT_REPOSITORY https://example.com/assetc.git
#       GIT_TAG        <tag>
#       SOURCE_SUBDIR  src/sdk)        # point at this folder, skip the parent project
#   FetchContent_MakeAvailable(assetc_sdk)
#   target_link_libraries(my_engine PRIVATE assetc::sdk)
#
# It links nothing but the C++23 standard library.

cmake_minimum_required(VERSION 3.20)

project(assetc_sdk LANGUAGES CXX)

add_library(assetc_sdk STATIC
    src/runtime_mesh.cpp
    src/runtime_material.cpp
    src/runtime_manifest.cpp
    src/runtime_anim.cpp
    src/runtime_font.cpp
    src/pack.cpp
    src/hash.cpp
    src/diag.cpp
)
add_library(assetc::sdk ALIAS assetc_sdk)

target_compile_features(assetc_sdk PUBLIC cxx_std_23)
set_target_properties(assetc_sdk PROPERTIES CXX_EXTENSIONS OFF)

# Public headers live under include/ so consumers write `#include <assetc/sdk.hpp>`.
# The private impl headers (src/diag.hpp) are reachable only from this target's
# own sources, never exported.
target_include_directories(assetc_sdk
    PUBLIC
        $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
        $<INSTALL_INTERFACE:include>
    PRIVATE
        ${CMAKE_CURRENT_SOURCE_DIR}/src
)

# Optional install rules: only emitted when the SDK is the top-level project, so
# they don't pollute the parent build's install set.
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    include(GNUInstallDirs)
    install(TARGETS assetc_sdk EXPORT assetc_sdk-targets
        ARCHIVE DESTINATION ${CMAKE_INSTALL_LIBDIR})
    install(DIRECTORY include/ DESTINATION ${CMAKE_INSTALL_INCLUDEDIR})
    install(EXPORT assetc_sdk-targets
        NAMESPACE assetc::
        DESTINATION ${CMAKE_INSTALL_LIBDIR}/cmake/assetc_sdk)
endif()
