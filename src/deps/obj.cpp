#include "obj.hpp"

#include "fmt.hpp"
#include <filesystem>

#define TINYOBJLOADER_IMPLEMENTATION
#include "tiny/tiny_obj_loader.h"

std::unique_ptr<obj::OBJ> obj::Load(const std::string &path)
{
    tinyobj::ObjReaderConfig config;
    config.mtl_search_path = std::filesystem::path(path).parent_path().string();

    tinyobj::ObjReader reader;
    if (!reader.ParseFromFile(path, config))
    {
        if (!reader.Error().empty())
            fmtx::Error(fmt::format("tinyobj: {}", reader.Error()));
        return nullptr;
    }

    if (!reader.Warning().empty())
        fmtx::Warn(fmt::format("tinyobj: {}", reader.Warning()));

    auto out = std::make_unique<OBJ>();
    out->attrib    = reader.GetAttrib();
    out->shapes    = reader.GetShapes();
    out->materials = reader.GetMaterials();
    return out;
}
