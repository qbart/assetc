set(SLANG_INSTALL_DIR ${CMAKE_BINARY_DIR}/slang-install)

ExternalProject_Add(
    slang_external
    GIT_REPOSITORY    https://github.com/shader-slang/slang.git
    GIT_TAG           v2026.8.1
    GIT_SHALLOW       TRUE

    PREFIX            ${CMAKE_BINARY_DIR}/slang
    INSTALL_DIR       ${SLANG_INSTALL_DIR}

    CMAKE_ARGS
        -DCMAKE_INSTALL_PREFIX=<INSTALL_DIR>
        -DBUILD_SHARED_LIBS=OFF
        -DSLANG_LIB_TYPE=STATIC
        -DCMAKE_BUILD_TYPE=Release
        -DCMAKE_POSITION_INDEPENDENT_CODE=ON
        -DSLANG_ENABLE_SLANG_GLSLANG=OFF
        -DSLANG_ENABLE_TESTS=OFF
        -DSLANG_ENABLE_EXAMPLES=OFF
        -DSLANG_ENABLE_REPLAYER=OFF
        -DSLANG_ENABLE_GFX=OFF
        -DSLANG_ENABLE_SLANGD=OFF
        -DSLANG_ENABLE_SLANGRT=OFF
        -DSLANG_ENABLE_SLANGC=OFF
        -DSLANG_EMBED_CORE_MODULE=ON
        -DSLANG_ENABLE_CUDA=OFF
        -DSLANG_ENABLE_OPTIX=OFF
        -DSLANG_ENABLE_NVAPI=OFF
        -DSLANG_ENABLE_AFTERMATH=OFF
        -DSLANG_ENABLE_SLANG_RHI=OFF
        -DSLANG_ENABLE_SLANGI=OFF
        -DSLANG_ENABLE_PREBUILT_BINARIES=OFF

  BUILD_COMMAND
      ${CMAKE_COMMAND} --build <BINARY_DIR> --config Release
          --target slang
          --target slang-embedded-core-module

    BUILD_BYPRODUCTS
        <INSTALL_DIR>/lib/${CMAKE_SHARED_LIBRARY_PREFIX}slang${CMAKE_SHARED_LIBRARY_SUFFIX}

    UPDATE_DISCONNECTED TRUE
)

add_library(slang SHARED IMPORTED GLOBAL)
add_dependencies(slang slang_external)
file(MAKE_DIRECTORY ${SLANG_INSTALL_DIR}/include)
set_target_properties(slang PROPERTIES
  IMPORTED_LOCATION
      ${SLANG_INSTALL_DIR}/lib/${CMAKE_SHARED_LIBRARY_PREFIX}slang${CMAKE_SHARED_LIBRARY_SUFFIX}
  INTERFACE_INCLUDE_DIRECTORIES
      ${SLANG_INSTALL_DIR}/include
)

# FetchContent_Declare(
#     slanglib
#     QUIET
#     GIT_REPOSITORY    https://github.com/shader-slang/slang.git
#     GIT_TAG           v2026.8.1
# )
# FetchContent_MakeAvailable(slanglib)
