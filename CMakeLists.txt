cmake_minimum_required(VERSION 4.3.2)

include(FetchContent)

set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(UNIX AND NOT APPLE)
    set(LINUX TRUE)
endif()

project(assetc)

list(APPEND CMAKE_MODULE_PATH ${CMAKE_CURRENT_SOURCE_DIR}/cmake)

add_executable(${PROJECT_NAME}
    src/deps/fmt.cpp
    src/deps/stb.cpp
    src/deps/ktx.cpp
    src/deps/gltf.cpp
    src/deps/obj.cpp
    src/deps/tiny/mikktspace.c
    src/assetc/shader.cpp
    src/assetc/runtime_mesh.cpp
    src/assetc/encode_mesh.cpp
    src/main.cpp
    )


set_target_properties(${PROJECT_NAME} PROPERTIES
            CXX_STANDARD 23
            CXX_EXTENSIONS OFF
            )


include(ktx)
include(cli11)
include(fmt)
include(meshoptimizer)
include(slang)

if(APPLE)
    set(CMAKE_CXX_FLAGS "${CMAKE_CXX_FLAGS} -stdlib=libc++")

    find_library(COCOA_LIBRARY Cocoa REQUIRED)
    find_library(IOKIT_LIBRARY IOKit REQUIRED)
    find_library(COREVIDEO_LIBRARY CoreVideo REQUIRED)
    find_library(CARBON Carbon REQUIRED)

    target_link_libraries(${PROJECT_NAME} PRIVATE
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
elseif (LINUX)
    target_link_libraries(${PROJECT_NAME} PRIVATE
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
    )
else()
    # For non-macOS systems, you can specify general linking here
    target_link_libraries(${PROJECT_NAME} PRIVATE
        fmt::fmt
        CLI11::CLI11
        ktx
        meshoptimizer
        slang
    )
endif()

