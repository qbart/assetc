cmake_minimum_required(VERSION 4.3.2)

include(FetchContent)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(UNIX AND NOT APPLE)
    set(LINUX TRUE)
endif()

project(assetc)

list(APPEND CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/cmake)

include(ktx)
include(cli11)
include(fmt)
include(meshoptimizer)
include(slang)

# All compilation units except main.cpp live in a static core library so both the
# `assetc` executable and the ctest binaries can link the same logic.
add_library(assetc_core STATIC
    src/deps/fmt.cpp
    src/deps/stb.cpp
    src/deps/ktx.cpp
    src/deps/gltf.cpp
    src/deps/obj.cpp
    src/deps/tiny/mikktspace.c
    src/assetc/shader.cpp
    src/assetc/runtime_mesh.cpp
    src/assetc/runtime_material.cpp
    src/assetc/runtime_manifest.cpp
    src/assetc/inspect.cpp
    src/assetc/cache.cpp
    src/assetc/encode_mesh.cpp
    src/assetc/encode_material.cpp
    src/assetc/encode_lut.cpp
    )

set_target_properties(assetc_core PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )

# `src` on the include path lets consumers (and tests) use `#include "assetc/..."`.
target_include_directories(assetc_core PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/src)

if(APPLE)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")

    find_library(COCOA_LIBRARY Cocoa REQUIRED)
    find_library(IOKIT_LIBRARY IOKit REQUIRED)
    find_library(COREVIDEO_LIBRARY CoreVideo REQUIRED)
    find_library(CARBON Carbon REQUIRED)

    target_link_libraries(assetc_core PUBLIC
        ${COCOA_LIBRARY}
        ${IOKIT_LIBRARY}
        ${COREVIDEO_LIBRARY}
        ${CARBON_LIBRARY}
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
    )
else()
    target_link_libraries(assetc_core PUBLIC
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
    )
endif()

add_executable(${PROJECT_NAME} src/main.cpp)
set_target_properties(${PROJECT_NAME} PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )
target_link_libraries(${PROJECT_NAME} PRIVATE assetc_core)

enable_testing()
add_subdirectory(tests)
