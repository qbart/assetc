#include "deps/fmt.hpp"
#include <CLI/CLI.hpp>
#include <filesystem>
#include <fmt/format.h>
#include <iostream>
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
        if (ext == ".n.png")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Normal});
        else if (ext == ".ao.png" || ext == ".h.png" || ext == ".r.png")
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

    for (const auto& a : assets) {
        fmtx::Info(fmt::format("Asset {}", a.path));
    }

    return 0;
}
