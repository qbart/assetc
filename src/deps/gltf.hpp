#pragma once

#include <memory>
#include <string>
#define TINYGLTF3_IMPLEMENTATION
#define TINYGLTF3_ENABLE_FS
#define TINYGLTF3_NOEXCEPTION
#include "stb.hpp"
#define TINYGLTF3_ENABLE_STB_IMAGE
#define TINYGLTF3_ENABLE_COROUTINES
#include "tiny/tiny_gltf_v3.h"

namespace gltf
{
struct GLTF
{
    tg3_model model;

    ~GLTF();
};

std::unique_ptr<GLTF> Load(const std::string &path);
} // namespace gltf
