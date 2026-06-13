cmake_minimum_required(VERSION 4.3.2)

include(FetchContent)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(UNIX AND NOT APPLE)
    set(LINUX TRUE)
endif()

project(assetc)

list(APPEND CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/cmake)

find_package(Vulkan)
include(ktx)
include(cli11)
include(fmt)
include(meshoptimizer)
include(slang)
include(yaml)
include(glfw)
include(imgui)

# Set the Apple stdlib flag before add_subdirectory so the SDK subdirectory builds
# with the same standard library this project uses (libc++, needed for std::format).
if(APPLE)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")
endif()

# The runtime loaders + binary format definitions live in a standalone,
# dependency-free SDK (src/sdk) that an engine can FetchContent on its own. Both
# this project and the SDK share that one definition of every on-disk format.
add_subdirectory(src/sdk)

# All compilation units except main.cpp live in a static core library so both the
# `assetc` executable and the ctest binaries can link the same logic. The runtime
# loaders are NOT listed here — they come from assetc_sdk below.
add_library(assetc_core STATIC
    src/deps/fmt.cpp
    src/deps/stb.cpp
    src/deps/ktx.cpp
    src/deps/gltf.cpp
    src/deps/obj.cpp
    src/deps/tiny/mikktspace.c
    src/assetc/shader.cpp
    src/assetc/inspect.cpp
    src/assetc/check.cpp
    src/assetc/cache.cpp
    src/assetc/config.cpp
    src/assetc/encode_mesh.cpp
    src/assetc/encode_material.cpp
    src/assetc/encode_lut.cpp
    src/assetc/encode_font.cpp
    )

set_target_properties(assetc_core PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )

# `src` on the include path lets consumers (and tests) use `#include "assetc/..."`.
target_include_directories(assetc_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)

if(APPLE)
    find_library(COCOA_LIBRARY Cocoa REQUIRED)
    find_library(IOKIT_LIBRARY IOKit REQUIRED)
    find_library(COREVIDEO_LIBRARY CoreVideo REQUIRED)
    find_library(CARBON Carbon REQUIRED)

    target_link_libraries(assetc_core PUBLIC
        ${COCOA_LIBRARY}
        ${IOKIT_LIBRARY}
        ${COREVIDEO_LIBRARY}
        ${CARBON_LIBRARY}
        assetc::sdk
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
        yaml-cpp::yaml-cpp
        Vulkan::Vulkan
    )
else()
    target_link_libraries(assetc_core PUBLIC
        assetc::sdk
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
        yaml-cpp::yaml-cpp
        Vulkan::Vulkan
    )
endif()

# The `assetc ui` inspector (Vulkan + GLFW + Dear ImGui). Kept in its own static
# lib so the windowing/GPU deps don't leak into the ctest binaries.
add_library(assetc_viewer STATIC
    src/viewer/gpu.cpp
    src/viewer/viewer.cpp
    src/viewer/mesh_view.cpp
    )
set_target_properties(assetc_viewer PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )
target_link_libraries(assetc_viewer PUBLIC assetc_core imgui glfw)

add_executable(${PROJECT_NAME} src/main.cpp)
set_target_properties(${PROJECT_NAME} PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )
target_link_libraries(${PROJECT_NAME} PRIVATE assetc_core assetc_viewer)

enable_testing()
add_subdirectory(tests)
