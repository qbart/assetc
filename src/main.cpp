#include "assetc/encode_lut.hpp"
#include "assetc/encode_material.hpp"
#include "assetc/encode_font.hpp"
#include "assetc/cache.hpp"
#include "assetc/check.hpp"
#include "assetc/config.hpp"
#include "assetc/encode_mesh.hpp"
#include "assetc/runtime_font.hpp"
#include "assetc/inspect.hpp"
#include "assetc/pack.hpp"
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"
#include "assetc/runtime_mesh.hpp"
#include "assetc/shader.hpp"
#include "viewer/viewer.hpp"
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
    LUT,
    Font
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
    case AssetType::Font:
        return "Font";
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

    // A shader mirrors its source `.shader` directory with the `.shader` suffix
    // stripped, into which one `<entryPoint>.spv` per Slang entry point is written.
    // e.g. assets/shaders/bloom.shader/ -> runtime/shaders/bloom/{vert,down,up}.spv
    if (type == AssetType::Shader)
    {
        rel.replace_extension();
        return (fs::path(outputDir) / rel).generic_string();
    }

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
    case AssetType::Font:
        // foo.ttf -> foo.hfont (+ companion foo.ktx2 SDF atlas written alongside)
        ext = ".hfont";
        break;
    }

    fs::path out = fs::path(outputDir) / rel;
    if (out.has_extension()) out.replace_extension();
    out += ext;
    return out.generic_string();
}

int handleAsset(const Asset &asset, const std::string &outputDir, unsigned threadCount,
                bool verify, std::vector<assetc::ManifestEntry> &outEntries,
                const assetc::Config &config, const std::string &preset)
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

        // `texture.compress: false` (e.g. UI/data textures) -> raw, pixel-exact.
        std::error_code   rec;
        const std::string rel = fs::relative(asset.path, config.input, rec).generic_string();
        if (!config.resolve(rel, preset).compress)
            return ktx::FromImageToRawKtx2(img, out, mode, threadCount);
        return ktx::FromImageToUASTC(img, out, mode, threadCount);
    }
    case AssetType::Mesh:
    {
        const auto         ext       = fs::path(asset.path).extension();
        const auto         sourceRef = asset.SourceRef();
        assetc::CompiledMesh cm;
        std::unique_ptr<gltf::GLTF> gltfSrc; // kept alive for companion .hmat + .ktx2 emission

        if (ext == ".obj")
        {
            auto src = obj::Load(asset.path);
            if (!src)
                return 1;
            cm = assetc::BuildFromObj(*src, sourceRef);
        }
        else if (ext == ".gltf" || ext == ".glb")
        {
            gltfSrc = gltf::Load(asset.path);
            if (!gltfSrc)
                return 1;
            // Per-pattern merge override, keyed by the source path relative to input.
            std::error_code  rec;
            const std::string rel = fs::relative(asset.path, config.input, rec).generic_string();
            const bool        merge = config.resolve(rel, preset).merge;
            cm = assetc::BuildFromGltf(*gltfSrc, sourceRef, merge);
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

        fmtx::Info(fmt::format("{} {} verts / {} tris / {} meshlets / {} submeshes / {} mats -> {}",
                               asset.type, cm.mesh.vertices.size(),
                               cm.mesh.indices.size() / 3, cm.mesh.meshlets.size(),
                               cm.submeshes.size(), cm.materialRefs.size(), out));
        if (int rc = assetc::WriteHMesh(out, cm.mesh, cm.bounds, cm.submeshes, cm.materialRefs,
                                        cm.skeleton);
            rc != 0)
            return rc;
        if (verify && assetc::ValidateHMesh(out) != 0)
            return 1;

        // Companion animation clips (.hanim) when the source is animated.
        if (!cm.animations.empty())
        {
            fs::path animPath = fs::path(out);
            animPath.replace_extension(".hanim");
            size_t channels = 0;
            for (const auto &c : cm.animations)
                channels += c.channels.size();
            fmtx::Info(fmt::format("{} clips / {} joints / {} channels -> {}",
                                   cm.animations.size(), cm.skeleton.size(), channels,
                                   animPath.generic_string()));
            if (assetc::WriteHAnim(animPath.generic_string(), cm.animations) != 0)
                return 1;
            if (verify && assetc::ValidateHAnim(animPath.generic_string()) != 0)
                return 1;
        }

        // Companion material table + textures (glTF only; OBJ materials are TODO).
        // One .hmat per source, indexed by slot (row i == SubMesh::materialSlot i);
        // each referenced glTF image becomes its own .ktx2 under <basename>/tex_<i>.ktx2.
        if (gltfSrc && !cm.materialSourceIndex.empty())
        {
            const auto mats =
                assetc::BuildMaterialsFromGltf(*gltfSrc, cm.materialSourceIndex, sourceRef);

            fs::path hmatPath = fs::path(out);
            hmatPath.replace_extension(".hmat");
            fs::path texDir = fs::path(out);
            texDir.replace_extension(); // <outputDir>/<rel-no-ext>/ holds tex_<i>.ktx2

            fmtx::Info(fmt::format("{} {} materials / {} textures -> {}", AssetType::Material,
                                   mats.rows.size(), mats.textures.size(),
                                   hmatPath.generic_string()));

            if (assetc::WriteHMat(hmatPath.generic_string(), mats.rows) != 0)
                return 1;
            if (verify && assetc::ValidateHMat(hmatPath.generic_string()) != 0)
                return 1;

            // texture.compress for this source (a UI/data glTF can opt all its
            // images out of block compression via a rule on the .glb path).
            std::error_code   trec;
            const std::string trel = fs::relative(asset.path, config.input, trec).generic_string();
            const bool        texCompress = config.resolve(trel, preset).compress;

            fs::create_directories(texDir);
            for (const auto &t : mats.textures)
            {
                const auto texPath =
                    (texDir / fmt::format("tex_{}.ktx2", t.imageIndex)).generic_string();
                if (assetc::EncodeGltfImageToKtx2(*gltfSrc, t.imageIndex, t.mode, texPath,
                                                  threadCount, texCompress) != 0)
                    return 1;

                // Runtime-relative path so the engine resolves root + "/" + path.
                const auto relPath =
                    fs::path(texPath).lexically_relative(outputDir).generic_string();
                // sRGB only for the color slot; ORM/normal/grayscale are sampled linear.
                const auto cs = t.mode == ktx::UASTCMode::Color ? assetc::ManColorSpace::Srgb
                                                                : assetc::ManColorSpace::Linear;
                outEntries.push_back(assetc::ManifestEntry{t.refHash, assetc::ManKind::Texture, cs,
                                                           relPath});
            }
        }
        return 0;
    }
    case AssetType::Shader:
    {
        fmtx::Info(fmt::format("{} {} -> {}", asset.type, asset.path, out));
        std::vector<std::string> entryPoints;
        if (int rc = assetc::CompileShaderFolder(asset.path, out, &entryPoints); rc != 0)
            return rc;

        // Register each emitted entry point so the engine can resolve it by hash
        // like a texture: ref "<sourceRef>/<entryPoint>", on-disk path that + ".spv".
        const auto sourceRef = asset.SourceRef();
        for (const auto &name : entryPoints)
        {
            const auto ref     = sourceRef + "/" + name;
            const auto relPath = ref + ".spv";
            outEntries.push_back(assetc::ManifestEntry{assetc::HashAssetRef(ref),
                                                       assetc::ManKind::Shader,
                                                       assetc::ManColorSpace::Linear, relPath});
        }
        return 0;
    }
    case AssetType::LUT:
    {
        fmtx::Info(fmt::format("{} {} -> {}", asset.type, asset.path, out));
        return assetc::EncodeLutCube(asset.path, out);
    }
    case AssetType::Font:
    {
        // foo.ttf -> foo.hfont (metrics) + foo.ktx2 (SDF atlas). The atlas is hashed
        // by the font's source ref and registered in the manifest so the .hfont's
        // atlasTex resolves the same way a material's texture ref does.
        const auto sourceRef = asset.SourceRef();
        fs::path   atlas     = fs::path(out);
        atlas.replace_extension(".ktx2");

        fmtx::Info(fmt::format("{} {} -> {}", asset.type, asset.path, out));
        if (assetc::EncodeFont(asset.path, out, atlas.generic_string(), sourceRef, threadCount) != 0)
            return 1;
        if (verify && assetc::ValidateHFont(out) != 0)
            return 1;

        const auto relPath = fs::path(atlas).lexically_relative(outputDir).generic_string();
        outEntries.push_back(assetc::ManifestEntry{assetc::HashAssetRef(sourceRef),
                                                   assetc::ManKind::Texture,
                                                   assetc::ManColorSpace::Linear, relPath});
        return 0;
    }
    case AssetType::Cubemap:
    {
        // .env/ directory holds 6 face PNGs in libktx face order:
        //   px.png nx.png py.png ny.png pz.png nz.png   (+X, -X, +Y, -Y, +Z, -Z)
        static constexpr const char *kFaceNames[6] = {"px", "nx", "py", "ny", "pz", "nz"};
        stb::Image faces[6];
        int faceSize = 0;
        for (int i = 0; i < 6; ++i)
        {
            const auto facePath =
                (fs::path(asset.path) / fmt::format("{}.png", kFaceNames[i])).generic_string();
            faces[i] = stb::Load(facePath);
            if (!faces[i].pixels)
            {
                fmtx::Error(
                    fmt::format("cubemap face load failed: {}: {}", facePath, stb::ImageError()));
                return 1;
            }
            if (faces[i].w != faces[i].h)
            {
                fmtx::Error(fmt::format("cubemap face must be square: {} ({}x{})", facePath,
                                        faces[i].w, faces[i].h));
                return 1;
            }
            if (i == 0)
                faceSize = faces[i].w;
            else if (faces[i].w != faceSize)
            {
                fmtx::Error(fmt::format("cubemap face size mismatch: {} ({}x{} vs {}x{})", facePath,
                                        faces[i].w, faces[i].h, faceSize, faceSize));
                return 1;
            }
        }

        fmtx::Info(fmt::format("{} {}x{}x6 -> {}", asset.type, faceSize, faceSize, out));
        return ktx::FromCubemapToUASTC(faces, out, threadCount);
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
    bool        verify     = false;
    bool        noCache     = false;
    bool        pack        = false;
    bool        listPresets = false;
    std::string presetArg;
    app.add_option("-j,--jobs", jobs, "Concurrent jobs")->check(CLI::PositiveNumber);
    auto       *outputOpt =
        app.add_option("-o,--output", outputDir, "Output directory")->capture_default_str();
    app.add_option("--preset", presetArg, "Use a named preset from assetc.yml");
    app.add_flag("--list-presets", listPresets, "List presets defined in assetc.yml and exit");
    app.add_flag("--verify", verify, "Re-read each written .hmesh and check structural validity");
    app.add_flag("--no-cache", noCache, "Ignore the incremental build cache and rebuild everything");
    app.add_flag("--pack", pack, "After building, bundle the output dir into a single <output>.hpack");
    auto *initCmd = app.add_subcommand("init", "Write a starter assetc.yml in the current directory");
    auto *infoCmd =
        app.add_subcommand("info", "Inspect compiled output in the output dir and print stats");
    infoCmd->fallthrough(); // let `info -o <dir>` reach the parent's --output option
    auto *checkCmd =
        app.add_subcommand("check", "Verify cross-file integrity of the compiled output dir");
    checkCmd->fallthrough();
    auto       *uiCmd =
        app.add_subcommand("ui", "Open the ImGui asset inspector on a .hpack or output dir");
    std::string uiPath;
    uiCmd->add_option("path", uiPath, "Path to a .hpack or compiled output dir (default: <output>)");
    uiCmd->fallthrough();
    auto       *packCmd = app.add_subcommand("pack", "Pack operations");
    std::string packInfoFile;
    auto       *packInfoCmd = packCmd->add_subcommand("info", "Inspect a .hpack and list its contents");
    packInfoCmd->add_option("file", packInfoFile, "Path to the .hpack (default: <output>.hpack)");
    packInfoCmd->fallthrough();
    packCmd->require_subcommand(1); // bare `pack` -> show its subcommands

    CLI11_PARSE(app, argc, argv);

    // `assetc init`: drop a starter assetc.yml in the current dir (never clobber)
    // and scaffold the source tree so the first build has somewhere to look.
    if (initCmd->parsed())
    {
        const std::string cfgPath = "assetc.yml";
        if (fs::exists(cfgPath))
        {
            fmtx::Error(fmt::format("{} already exists; not overwriting", cfgPath));
            return 1;
        }
        if (assetc::WriteDefaultConfig(cfgPath) != 0)
            return 1;
        fmtx::Success(fmt::format("wrote {}", cfgPath));

        // Create the source tree (the starter config's `input:`) if it's missing.
        const std::string inputDir = assetc::Config{}.input;
        if (!fs::exists(inputDir))
        {
            std::error_code ec;
            fs::create_directories(inputDir, ec);
            if (ec)
            {
                fmtx::Error(fmt::format("cannot create input dir {}: {}", inputDir, ec.message()));
                return 1;
            }
            fmtx::Success(fmt::format("created {}/", inputDir));
        }
        return 0;
    }

    // Project config (nearest assetc.yml, searching upward). CLI flags win over it.
    assetc::Config config;
    std::string    configPath;
    if (assetc::LoadConfig(fs::current_path().string(), config, configPath) != 0)
        return 1;
    if (!configPath.empty())
        fmtx::Info(fmt::format("config: {}", configPath));

    // Effective preset: --preset wins over the config's `preset:`.
    const std::string preset = !presetArg.empty() ? presetArg : config.preset;

    auto presetList = [&] {
        const auto names = config.presetNames();
        std::string s;
        for (size_t i = 0; i < names.size(); ++i)
            s += (i ? ", " : "") + names[i];
        return s.empty() ? std::string("(none)") : s;
    };

    if (listPresets)
    {
        fmtx::Info(fmt::format("presets: {}", presetList()));
        return 0;
    }
    if (!preset.empty() && !config.hasPreset(preset))
    {
        fmtx::Error(fmt::format("unknown preset '{}' (available: {})", preset, presetList()));
        return 1;
    }
    if (!preset.empty())
        fmtx::Info(fmt::format("preset: {}", preset));

    if (outputOpt->count() == 0)
        outputDir = config.outputFor(preset); // config/preset output unless -o was given
    pack = pack || config.pack;

    // `assetc info`: report on what's already in the output dir, don't recompile.
    if (infoCmd->parsed())
        return assetc::InspectRuntime(outputDir);

    // `assetc check`: validate the compiled output's internal consistency.
    if (checkCmd->parsed())
        return assetc::CheckRuntime(outputDir);

    // `assetc ui`: open the ImGui inspector. Default target is the output dir;
    // if that's absent but a sibling <output>.hpack exists, view the pack instead.
    if (uiCmd->parsed())
    {
        std::string target = uiPath;
        if (target.empty())
        {
            target = outputDir;
            if (!fs::exists(target))
            {
                const fs::path od   = outputDir;
                const auto      pack = (od.parent_path() / (od.filename().string() + ".hpack"));
                if (fs::exists(pack))
                    target = pack.generic_string();
            }
        }
        return assetc::RunViewer(target);
    }

    // `assetc pack info`: inspect a .hpack (defaults to the output's sibling pack).
    if (packInfoCmd->parsed())
    {
        std::string pp = packInfoFile;
        if (pp.empty())
        {
            const fs::path od = outputDir;
            pp = (od.parent_path() / (od.filename().string() + ".hpack")).generic_string();
        }
        return assetc::InspectPack(pp);
    }

    fmtx::Info(fmt::format("Running {} threads", jobs));

    fs::path assetDir = config.input;
    if (!fs::exists(assetDir) || !fs::is_directory(assetDir))
    {
        fmtx::Error(fmt::format("Asset dir does not exist ({})", assetDir.generic_string()));
        return 1;
    }

    // Ensure the output dir exists up front: assets create their own subdirs
    // lazily, but the cache and manifest are written at the top level even when no
    // asset produced output there (e.g. an empty asset tree).
    std::error_code mkdirEc;
    fs::create_directories(outputDir, mkdirEc);
    if (mkdirEc)
    {
        fmtx::Error(fmt::format("cannot create output dir {}: {}", outputDir, mkdirEc.message()));
        return 1;
    }

    std::filesystem::recursive_directory_iterator iter(
        assetDir, std::filesystem::directory_options::skip_permission_denied
    );

    std::vector<Asset> assets;

    auto insideContainerDir = [](const fs::path &p) {
        for (auto cur = p.parent_path(); cur.has_relative_path(); cur = cur.parent_path())
        {
            auto fn = cur.filename().string();
            if (fn.ends_with(".env") || fn.ends_with(".array") || fn.ends_with(".shader"))
                return true;
        }
        return false;
    };

    for (const auto &entry : iter)
    {
        auto name = entry.path().string();
        auto ext  = entry.path().extension();
        if (entry.is_directory())
        {
            if (name.ends_with(".env"))
                assets.emplace_back(Asset{.path = name, .type = AssetType::Cubemap});
            else if (name.ends_with(".array"))
                assets.emplace_back(Asset{.path = name, .type = AssetType::Array});
            else if (name.ends_with(".shader"))
                assets.emplace_back(Asset{.path = name, .type = AssetType::Shader});
            continue;
        }

        // Files inside .env/ .array/ .shader/ containers are consumed by the
        // container's encoder; don't pick them up as standalone assets.
        if (insideContainerDir(entry.path()))
            continue;

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
        else if (ext == ".ttf" || ext == ".otf")
            assets.emplace_back(Asset{.path = name, .type = AssetType::Font});
    }

    if (assets.empty())
        fmtx::Warn(fmt::format("no compilable assets found under {} (writing empty manifest)",
                               assetDir.generic_string()));
    else
        fmtx::Info(fmt::format("found {} asset(s) under {}", assets.size(),
                               assetDir.generic_string()));

    // Split the `jobs` thread budget between outer (across assets) and inner
    // (within each ktx encode), based on how much real work there is.
    //
    //   24 jobs, 3 image assets  -> 3 outer * 8 inner = 24 threads used
    //   24 jobs, 100 image assets -> 24 outer * 1 inner = 24 threads used
    //   24 jobs, 1 image asset   -> 1 outer * 24 inner = 24 threads used
    const size_t imageCount = std::count_if(assets.begin(), assets.end(), [](const Asset &a) {
        return a.type == AssetType::Color || a.type == AssetType::Normal ||
               a.type == AssetType::Grayscale || a.type == AssetType::Font;
    });

    const unsigned outerWorkers = std::min<unsigned>(jobs, std::max<size_t>(1, imageCount));
    const unsigned innerThreads = std::max(1u, jobs / outerWorkers);
    fmtx::Info(fmt::format("schedule: {} outer x {} inner = {} threads",
                           outerWorkers, innerThreads, outerWorkers * innerThreads));

    std::mutex logMu;
    std::atomic<size_t> next{0};
    std::atomic<int> failures{0};
    std::atomic<int> cached{0};
    assetc::ManifestSink manifest;

    // Incremental build: skip an asset whose source bytes + settings are unchanged
    // and whose primary output still exists, replaying its cached manifest entries.
    const auto cachePath = (fs::path(outputDir) / ".assetc-cache").generic_string();
    const assetc::CacheTable oldCache = noCache ? assetc::CacheTable{} : assetc::LoadCache(cachePath);
    std::mutex                    cacheMu;
    std::vector<assetc::CacheEntry> newCache;

    auto worker = [&] {
        for (;;)
        {
            size_t i = next.fetch_add(1, std::memory_order_relaxed);
            if (i >= assets.size()) return;
            const Asset &a   = assets[i];
            const auto   out = a.RuntimePath(outputDir);
            // Fold the resolved settings + active preset into the cache seed so a
            // config/preset change (merge or compress flip) rebuilds affected assets.
            std::error_code   sec;
            const std::string rel = fs::relative(a.path, config.input, sec).generic_string();
            const auto        rs  = config.resolve(rel, preset);
            uint64_t          seed = static_cast<uint64_t>(a.type);
            seed = seed * 1000003u + (rs.merge ? 1u : 0u);
            seed = seed * 1000003u + (rs.compress ? 2u : 0u);
            for (char c : preset)
                seed = seed * 131u + static_cast<uint8_t>(c);
            const uint64_t inputHash = assetc::HashSource(a.path, seed);

            if (!noCache && inputHash != 0)
            {
                auto it = oldCache.find(a.path);
                if (it != oldCache.end() && it->second.inputHash == inputHash && fs::exists(out))
                {
                    for (const auto &m : it->second.manifest)
                        manifest.Add(m);
                    {
                        std::lock_guard<std::mutex> lk(cacheMu);
                        newCache.push_back(it->second);
                    }
                    cached.fetch_add(1, std::memory_order_relaxed);
                    continue;
                }
            }

            std::vector<assetc::ManifestEntry> entries;
            if (handleAsset(a, outputDir, innerThreads, verify, entries, config, preset) != 0)
            {
                std::lock_guard<std::mutex> lk(logMu);
                fmtx::Error(fmt::format("failed: {}", a.path));
                failures.fetch_add(1, std::memory_order_relaxed);
                continue;
            }
            for (const auto &m : entries)
                manifest.Add(m);
            if (inputHash != 0)
            {
                std::lock_guard<std::mutex> lk(cacheMu);
                newCache.push_back(assetc::CacheEntry{inputHash, a.path, std::move(entries)});
            }
        }
    };

    std::vector<std::thread> workers;
    workers.reserve(outerWorkers);
    for (unsigned i = 0; i < outerWorkers; ++i)
        workers.emplace_back(worker);
    for (auto &t : workers)
        t.join();

    // Persist the cache for all assets we trust this run (hits + fresh successes);
    // failed assets are absent so they retry next time.
    assetc::WriteCache(cachePath, newCache);

    // Always summarize the run so a no-op (nothing matched / all cached) is never
    // mistaken for real work.
    const int total  = static_cast<int>(assets.size());
    const int failed = failures.load();
    const int hit    = cached.load();
    const int built  = total - hit - failed;
    fmtx::Info(fmt::format("done: {} asset(s) -> {} built, {} cached, {} failed -> {}", total, built,
                           hit, failed, outputDir));

    if (failed != 0)
        return 1;

    // Single global hash -> file manifest, written only after every asset succeeded
    // so we never emit a partial/ambiguous table.
    const auto manPath = (fs::path(outputDir) / "assets.hman").generic_string();
    auto       entries = manifest.Take();
    fmtx::Info(fmt::format("{} asset refs -> {}", entries.size(), manPath));
    if (assetc::WriteHMan(manPath, std::move(entries)) != 0)
        return 1;
    if (verify && assetc::ValidateHMan(manPath) != 0)
        return 1;

    // --pack: bundle the whole output dir into a single <output>.hpack sibling.
    if (pack)
    {
        const fs::path od       = outputDir;
        const fs::path packPath = od.parent_path() / (od.filename().string() + ".hpack");
        if (assetc::WritePack(outputDir, packPath.generic_string()) != 0)
            return 1;
        if (verify && assetc::ValidatePack(packPath.generic_string()) != 0)
            return 1;
    }

    return 0;
}
