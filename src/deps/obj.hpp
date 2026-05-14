#pragma once

#include <memory>
#include <string>

#include "tiny/tiny_obj_loader.h"

namespace obj
{
struct OBJ
{
    tinyobj::attrib_t attrib;
    std::vector<tinyobj::shape_t> shapes;
    std::vector<tinyobj::material_t> materials;
};

std::unique_ptr<OBJ> Load(const std::string &path);
} // namespace obj
