#pragma once

#include "tinygltf/tiny_gltf_v3.h"
#include <memory>
#include <string>

namespace gltf
{
struct GLTF
{
    tg3_model model;

    ~GLTF();
};

std::unique_ptr<GLTF> Load(const std::string &path);
} // namespace gltf
