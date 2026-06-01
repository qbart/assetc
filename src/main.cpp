#include "assetc/encode_lut.hpp"
#include "assetc/encode_mesh.hpp"
#include "assetc/runtime_mesh.hpp"
#include "assetc/shader.hpp"
#include "deps/fmt.hpp"
#include "deps/gltf.hpp"
#include "deps/ktx.hpp"
#include "deps/obj.hpp"
#include "deps/stb.hpp"
#include <CLI/CLI.hpp>
#include <algorithm>
#include <atomic>
#include <filesystem>
#include <fmt/core.h>
#include <fmt/format.h>
#include <iostream>
#include <mutex>
#include <string_view>
#include <thread>
#include <vector>

#include <bit>
#include <ktx.h>
#include <thread>
#include <vulkan/vulkan.hpp>

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
    Material,
    LUT
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
    case AssetType::LUT:
        return "LUT";
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

    std::string RuntimePath(const std::string &outputDir) const;
    std::string SourceRef() const; // canonical file-level namespace ref (lowercase, no ext)
};

std::string Asset::SourceRef() const
{
    fs::path rel = fs::path(path).lexically_relative("assets");
    if (rel.empty())
        rel = fs::path(path);
    rel.replace_extension();
    std::string s = rel.generic_string();
    for (char &c : s)
    {
        if (c >= 'A' && c <= 'Z')
            c = static_cast<char>(c - 'A' + 'a');
    }
    return s;
}

std::string Asset::RuntimePath(const std::string &outputDir) const
{
    fs::path rel = fs::path(path).lexically_relative("assets");

    // A shader mirrors its source `.shader` directory: the runtime path is the
    // folder itself (extension kept), into which vertex.spv / fragment.spv go.
    if (type == AssetType::Shader)
        return (fs::path(outputDir) / rel).generic_string();

    std::string_view ext;
    switch (type)
    {
    case AssetType::Grayscale:
    case AssetType::Normal:
    case AssetType::Color:
        ext = ".ktx2";
        break;
    case AssetType::Array:
        ext = ".arr.ktx2";
        break;
    case AssetType::Cubemap:
        ext = ".env.ktx2";
        break;
    case AssetType::Shader:
        break; // handled above
    case AssetType::Mesh:
        ext = ".hmesh";
        break;
    case AssetType::Material:
        ext = ".hmat";
        break;
    case AssetType::LUT:
        // foo.lut.cube -> foo.lut.ktx2 (the .cube is stripped below, .ktx2 added)
        ext = ".ktx2";
        break;
    }

    fs::path out = fs::path(outputDir) / rel;
    if (out.has_extension()) out.replace_extension();
    out += ext;
    return out.generic_string();
}

int handleAsset(const Asset &asset, const std::string &outputDir, unsigned threadCount,
                bool verify)
{
    const auto out = asset.RuntimePath(outputDir);
    fs::create_directories(fs::path(out).parent_path());

    switch (asset.type)
    {
    case AssetType::Color:
    case AssetType::Normal:
    case AssetType::Grayscale:
    {
        stb::Image img = stb::Load(asset.path);
        if (!img.pixels)
        {
            fmtx::Error(fmt::format("load failed: {}: {}", asset.path, stb::ImageError()));
            return 1;
        }
        fmtx::Info(
            fmt::format("{} {}x{}x{} -> {}", asset.type, img.w, img.h, img.channels, out));

        ktx::UASTCMode mode = ktx::UASTCMode::Color;
        if (asset.type == AssetType::Normal)    mode = ktx::UASTCMode::Normal;
        if (asset.type == AssetType::Grayscale) mode = ktx::UASTCMode::Grayscale;
        return ktx::FromImageToUASTC(img, out, mode, threadCount);
    }
    case AssetType::Mesh:
    {
        const auto         ext       = fs::path(asset.path).extension();
        const auto         sourceRef = asset.SourceRef();
        assetc::CompiledMesh cm;

        if (ext == ".obj")
        {
            auto src = obj::Load(asset.path);
            if (!src)
                return 1;
            cm = assetc::BuildFromObj(*src, sourceRef);
        }
        else if (ext == ".gltf" || ext == ".glb")
        {
            auto src = gltf::Load(asset.path);
            if (!src)
                return 1;
            cm = assetc::BuildFromGltf(*src, sourceRef);
        }
        else
        {
            fmtx::Info(fmt::format("skip {} ({} not yet supported)", asset.path, ext.string()));
            return 0;
        }

        if (cm.mesh.vertices.empty())
        {
            fmtx::Error(fmt::format("encode failed: {}", asset.path));
            return 1;
        }

        fmtx::Info(fmt::format("{} {} verts / {} tris / {} meshlets / {} mats -> {}",
                               asset.type, cm.mesh.vertices.size(),
                               cm.mesh.indices.size() / 3, cm.mesh.meshlets.size(),
                               cm.materialRefs.size(), out));
        if (int rc = assetc::WriteHMesh(out, cm.mesh, cm.bounds, cm.materialRefs); rc != 0)
            return rc;
        if (verify)
            return assetc::ValidateHMesh(out);
        return 0;
    }
    case AssetType::Shader:
    {
        fmtx::Info(fmt::format("{} {} -> {}", asset.type, asset.path, out));
        return assetc::CompileShaderFolder(asset.path, out);
    }
    case AssetType::LUT:
    {
        fmtx::Info(fmt::format("{} {} -> {}", asset.type, asset.path, out));
        return assetc::EncodeLutCube(asset.path, out);
    }
    default:
        fmtx::Info(fmt::format("skip {} ({} not yet supported)", asset.path, asset.type));
        return 0;
    }
}

int main(int argc, char **argv)
{
    auto jobs = std::thread::hardware_concurrency();
    if (jobs == 0) jobs = 4;

    CLI::App app{"Asset compiler"};
    argv = app.ensure_utf8(argv);

    std::string outputDir = "runtime";
    bool        verify    = false;
    app.add_option("-j,--jobs", jobs, "Concurrent jobs")->check(CLI::PositiveNumber);
    app.add_option("-o,--output", outputDir, "Output directory")->capture_default_str();
    app.add_flag("--verify", verify, "Re-read each written .hmesh and check structural validity");
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
                assets.emplace_back(Asset{.path = name, .type = AssetType::Cubemap});
            }
            else if (name.ends_with(".array"))
            {
                assets.emplace_back(Asset{.path = name, .type = AssetType::Array});
            }
            else if (name.ends_with(".shader"))
            {
                assets.emplace_back(Asset{.path = name, .type = AssetType::Shader});
            }
            else
                continue;
        }
        if (name.ends_with(".lut.cube"))
            assets.emplace_back(Asset{.path = name, .type = AssetType::LUT});
        else if (name.ends_with(".n.png"))
            assets.emplace_back(Asset{.path = name, .type = AssetType::Normal});
        else if (name.ends_with(".ao.png") || name.ends_with(".h.png") || name.ends_with(".r.png"))
            assets.emplace_back(Asset{.path = name, .type = AssetType::Grayscale});
        else if (ext == ".png")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Color});
        else if (ext == ".obj")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Mesh});
        else if (ext == ".gltf")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Mesh});
        else if (ext == ".glb")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Mesh});
        else if (ext == ".mat")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Material});
    }

    // Split the `jobs` thread budget between outer (across assets) and inner
    // (within each ktx encode), based on how much real work there is.
    //
    //   24 jobs, 3 image assets  -> 3 outer * 8 inner = 24 threads used
    //   24 jobs, 100 image assets -> 24 outer * 1 inner = 24 threads used
    //   24 jobs, 1 image asset   -> 1 outer * 24 inner = 24 threads used
    const size_t imageCount = std::count_if(assets.begin(), assets.end(), [](const Asset &a) {
        return a.type == AssetType::Color || a.type == AssetType::Normal ||
               a.type == AssetType::Grayscale;
    });

    const unsigned outerWorkers = std::min<unsigned>(jobs, std::max<size_t>(1, imageCount));
    const unsigned innerThreads = std::max(1u, jobs / outerWorkers);
    fmtx::Info(fmt::format("schedule: {} outer x {} inner = {} threads",
                           outerWorkers, innerThreads, outerWorkers * innerThreads));

    std::mutex logMu;
    std::atomic<size_t> next{0};
    std::atomic<int> failures{0};

    auto worker = [&] {
        for (;;)
        {
            size_t i = next.fetch_add(1, std::memory_order_relaxed);
            if (i >= assets.size()) return;
            if (handleAsset(assets[i], outputDir, innerThreads, verify) != 0)
            {
                std::lock_guard<std::mutex> lk(logMu);
                fmtx::Error(fmt::format("failed: {}", assets[i].path));
                failures.fetch_add(1, std::memory_order_relaxed);
            }
        }
    };

    std::vector<std::thread> workers;
    workers.reserve(outerWorkers);
    for (unsigned i = 0; i < outerWorkers; ++i)
        workers.emplace_back(worker);
    for (auto &t : workers)
        t.join();

    return failures.load() == 0 ? 0 : 1;
}
