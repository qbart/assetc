#include "deps/fmt.hpp"
#include <CLI/CLI.hpp>
#include <filesystem>
#include <fmt/format.h>
#include <iostream>
#include <string_view>
#include <thread>
#include <vector>

namespace fs = std::filesystem;

enum class AssetType
{
    Grayscale,
    Normal,
    Color,
    Array,
    Cubemap,
    Shader,
    Mesh,
    Material
};

constexpr std::string_view to_string(AssetType t)
{
    switch (t)
    {
    case AssetType::Grayscale:
        return "Grayscale";
    case AssetType::Normal:
        return "Normal";
    case AssetType::Color:
        return "Color";
    case AssetType::Array:
        return "Array";
    case AssetType::Cubemap:
        return "Cubemap";
    case AssetType::Shader:
        return "Shader";
    case AssetType::Mesh:
        return "Mesh";
    case AssetType::Material:
        return "Material";
    }
    return "???"; // unreachable if enum is exhaustive
}

template <> struct fmt::formatter<AssetType> : fmt::formatter<std::string_view>
{
    auto format(AssetType t, format_context &ctx) const
    {
        return fmt::formatter<std::string_view>::format(to_string(t), ctx);
    }
};

struct Asset
{
    std::string path;
    AssetType type;
};

int main(int argc, char **argv)
{
    auto jobs = std::thread::hardware_concurrency();
    if (jobs == 0) jobs = 4;

    CLI::App app{"Asset compiler"};
    argv = app.ensure_utf8(argv);

    app.add_option("-j,--jobs", jobs, "Concurrent jobs")->check(CLI::PositiveNumber);
    app.add_subcommand("init", "Initialize structure");

    CLI11_PARSE(app, argc, argv);

    fmtx::Info(fmt::format("Running {} threads", jobs));

    fs::path assetDir = "assets";
    if (!fs::exists(assetDir) || !fs::is_directory(assetDir))
    {
        fmtx::Error("Asset dir does not exist (./assets)");
        return 1;
    }

    std::filesystem::recursive_directory_iterator iter(
        assetDir, std::filesystem::directory_options::skip_permission_denied
    );

    std::vector<Asset> assets;

    for (const auto &entry : iter)
    {
        auto name = entry.path().string();
        auto ext  = entry.path().extension();
        if (entry.is_directory())
        {
            if (name.ends_with(".env"))
            {
                assets.emplace_back(
                    Asset{
                        .path = name,
                        .type = AssetType::Cubemap,
                    }
                );
            }
            else if (name.ends_with(".array"))
            {
                assets.emplace_back(
                    Asset{
                        .path = name,
                        .type = AssetType::Array,
                    }
                );
            }
            else
                continue;
        }
        if (name.ends_with(".n.png"))
            assets.emplace_back(Asset{.path = name, .type = AssetType::Normal});
        else if (name.ends_with(".ao.png") || name.ends_with(".h.png") || name.ends_with(".r.png"))
            assets.emplace_back(Asset{.path = name, .type = AssetType::Grayscale});
        else if (ext == ".png")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Color});
        else if (ext == ".slang")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Shader});
        else if (ext == ".obj")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Mesh});
        else if (ext == ".mat")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Material});
    }

    for (const auto &a : assets)
    {
        fmtx::Info(fmt::format("Asset {} {}", a.type, a.path));
    }

    return 0;
}
