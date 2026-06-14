#include "check.hpp"

#include "assetc/hash.hpp" // HashAssetRef
#include "assetc/runtime_anim.hpp"
#include "assetc/runtime_manifest.hpp"
#include "assetc/runtime_material.hpp"
#include "assetc/runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <cstdint>
#include <filesystem>
#include <fstream>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

namespace fs = std::filesystem;

namespace
{

struct Ctx
{
    std::string                                root;
    std::unordered_map<uint64_t, std::string>  byHash; // manifest hash -> rel path
    int                                        problems = 0;
};

void Problem(Ctx &c, const std::string &where, const std::string &msg)
{
    fmt::print("  {}FAIL{} {}: {}\n", fmtx::RED, fmtx::RESET, where, msg);
    ++c.problems;
}

template <typename T> bool ReadAt(std::ifstream &in, uint64_t off, T &out)
{
    in.clear();
    in.seekg(static_cast<std::streamoff>(off));
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&out), sizeof(out)));
}

bool FileExists(const Ctx &c, const std::string &rel)
{
    std::error_code ec;
    return fs::exists(fs::path(c.root) / rel, ec);
}

// True if `ref` (a runtime ref without extension) names a content-store texture:
// "tex/<16 lowercase-hex digits>". On success `outHash` is the parsed hash.
bool IsContentStorePath(const std::string &ref, uint64_t &outHash)
{
    constexpr std::string_view prefix = "tex/";
    if (ref.size() != prefix.size() + 16 || ref.compare(0, prefix.size(), prefix) != 0)
        return false;
    uint64_t h = 0;
    for (size_t i = prefix.size(); i < ref.size(); ++i)
    {
        const char   ch = ref[i];
        unsigned     v;
        if (ch >= '0' && ch <= '9')
            v = static_cast<unsigned>(ch - '0');
        else if (ch >= 'a' && ch <= 'f')
            v = static_cast<unsigned>(ch - 'a' + 10);
        else
            return false;
        h = (h << 4) | v;
    }
    outHash = h;
    return true;
}

// ---- .hman -----------------------------------------------------------------

void CheckManifest(Ctx &c, const std::string &path, const std::string &rel)
{
    if (assetc::ValidateHMan(path) != 0)
    {
        Problem(c, rel, "structural validation failed");
        return;
    }
    std::vector<assetc::ManifestEntry> entries;
    if (assetc::ReadHMan(path, entries) != 0)
    {
        Problem(c, rel, "could not parse");
        return;
    }
    for (const auto &e : entries)
    {
        c.byHash.emplace(e.hash, e.path);
        if (!FileExists(c, e.path))
            Problem(c, rel, fmt::format("entry 0x{:016x} -> missing file \"{}\"", e.hash, e.path));
        // Texture-entry parity. Content-addressed textures live in the flat store as
        // "tex/<16-hex content hash>.ktx2", so their hash must equal the hex stem.
        // Name-addressed textures (e.g. font SDF atlases) keep the older invariant
        // hash == HashAssetRef(path without ".ktx2").
        if (e.kind == assetc::ManKind::Texture && e.path.ends_with(".ktx2"))
        {
            const auto ref = e.path.substr(0, e.path.size() - 5);
            uint64_t   stemHash = 0;
            if (IsContentStorePath(ref, stemHash))
            {
                if (stemHash != e.hash)
                    Problem(c, rel,
                            fmt::format("entry \"{}\" hash 0x{:016x} != content-store stem 0x{:016x}",
                                        e.path, e.hash, stemHash));
            }
            else
            {
                const uint64_t expected = assetc::HashAssetRef(ref);
                if (expected != e.hash)
                    Problem(
                        c, rel,
                        fmt::format("entry \"{}\" hash 0x{:016x} != HashAssetRef(\"{}\")=0x{:016x}",
                                    e.path, e.hash, ref, expected));
            }
        }
        // Path-addressed parity: embeds and the by-path assets (mesh/material/anim/
        // font) all key on HashEmbedRef of the full runtime path (extension kept).
        else if (e.kind == assetc::ManKind::Embed || e.kind == assetc::ManKind::Mesh ||
                 e.kind == assetc::ManKind::Material || e.kind == assetc::ManKind::Animation ||
                 e.kind == assetc::ManKind::Font)
        {
            const uint64_t expected = assetc::HashEmbedRef(e.path);
            if (expected != e.hash)
                Problem(c, rel,
                        fmt::format("entry \"{}\" hash 0x{:016x} != HashEmbedRef(\"{}\")=0x{:016x}",
                                    e.path, e.hash, e.path, expected));
        }
    }
}

// ---- .hmat -----------------------------------------------------------------

void CheckHMat(Ctx &c, const std::string &path, const std::string &rel)
{
    if (assetc::ValidateHMat(path) != 0)
    {
        Problem(c, rel, "structural validation failed");
        return;
    }
    std::ifstream         in(path, std::ios::binary);
    assetc::MatFileHeader hdr{};
    if (!in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
        return;

    for (uint32_t i = 0; i < hdr.count; ++i)
    {
        assetc::GpuMaterial m{};
        if (!in.read(reinterpret_cast<char *>(&m), sizeof(m)))
            break;
        const uint64_t refs[] = {m.baseColorTex, m.metallicRoughnessTex, m.normalTex,
                                 m.occlusionTex, m.emissiveTex};
        for (uint64_t ref : refs)
        {
            if (ref == 0)
                continue;
            auto it = c.byHash.find(ref);
            if (it == c.byHash.end())
                Problem(c, rel,
                        fmt::format("row {} texture ref 0x{:016x} not in manifest", i, ref));
            else if (!FileExists(c, it->second))
                Problem(c, rel,
                        fmt::format("row {} ref 0x{:016x} -> missing \"{}\"", i, ref, it->second));
        }
    }
}

// ---- .hmesh ----------------------------------------------------------------

void CheckHMesh(Ctx &c, const std::string &path, const std::string &rel)
{
    if (assetc::ValidateHMesh(path) != 0)
    {
        Problem(c, rel, "structural validation failed");
        return;
    }

    std::ifstream      in(path, std::ios::binary);
    assetc::FileHeader hdr{};
    if (!in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
        return;

    assetc::MeshDesc desc{};
    bool             haveDesc = false;
    uint64_t         submOff = 0, submSize = 0;
    for (uint32_t i = 0; i < hdr.chunkCount; ++i)
    {
        assetc::ChunkEntry e{};
        if (!ReadAt(in, sizeof(assetc::FileHeader) + static_cast<uint64_t>(i) * sizeof(e), e))
            break;
        if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Desc))
            haveDesc = ReadAt(in, e.offset, desc);
        else if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::SubMeshes))
        {
            submOff  = e.offset;
            submSize = e.size;
        }
    }
    if (!haveDesc)
        return; // ValidateHMesh already requires DESC; defensive.

    // Companion .hmat row count must equal the mesh's dense material count.
    if (desc.materialCount > 0)
    {
        auto hmat = fs::path(path);
        hmat.replace_extension(".hmat");
        if (!fs::exists(hmat))
            Problem(c, rel, fmt::format("{} materials but no companion {}", desc.materialCount,
                                        hmat.filename().string()));
        else
        {
            std::ifstream         mi(hmat, std::ios::binary);
            assetc::MatFileHeader mh{};
            if (mi.read(reinterpret_cast<char *>(&mh), sizeof(mh)) &&
                mh.count != desc.materialCount)
                Problem(c, rel,
                        fmt::format("material count {} != companion .hmat rows {}",
                                    desc.materialCount, mh.count));
        }
    }

    // Every submesh materialSlot is kNoMaterial or a valid index.
    const uint64_t n = submSize / sizeof(assetc::SubMesh);
    for (uint64_t i = 0; i < n; ++i)
    {
        assetc::SubMesh sm{};
        if (!ReadAt(in, submOff + i * sizeof(sm), sm))
            break;
        if (sm.materialSlot != assetc::kNoMaterial && sm.materialSlot >= desc.materialCount)
            Problem(c, rel,
                    fmt::format("submesh {} materialSlot {} out of range (materialCount {})", i,
                                sm.materialSlot, desc.materialCount));
    }
}

// Read the SKEL joint count from a .hmesh, or -1 if absent/unreadable.
int64_t SkeletonJointCount(const std::string &meshPath)
{
    std::ifstream      in(meshPath, std::ios::binary);
    assetc::FileHeader hdr{};
    if (!in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)) || hdr.magic != assetc::MeshMagic)
        return -1;
    for (uint32_t i = 0; i < hdr.chunkCount; ++i)
    {
        assetc::ChunkEntry e{};
        if (!ReadAt(in, sizeof(assetc::FileHeader) + static_cast<uint64_t>(i) * sizeof(e), e))
            break;
        if (e.fourcc == static_cast<uint32_t>(assetc::ChunkId::Skeleton))
            return static_cast<int64_t>(e.size / sizeof(assetc::GpuJoint));
    }
    return -1;
}

// ---- .hanim ----------------------------------------------------------------

void CheckHAnim(Ctx &c, const std::string &path, const std::string &rel)
{
    if (assetc::ValidateHAnim(path) != 0)
    {
        Problem(c, rel, "structural validation failed");
        return;
    }
    std::vector<assetc::AnimClip> clips;
    if (assetc::ReadHAnim(path, clips) != 0)
    {
        Problem(c, rel, "could not parse");
        return;
    }

    // The companion .hmesh must carry a skeleton the channels can target.
    auto mesh = fs::path(path);
    mesh.replace_extension(".hmesh");
    const int64_t joints = fs::exists(mesh) ? SkeletonJointCount(mesh.string()) : -1;
    if (joints < 0)
    {
        Problem(c, rel,
                fmt::format("no companion skeleton ({} with a SKEL chunk)",
                            mesh.filename().string()));
        return;
    }
    for (size_t ci = 0; ci < clips.size(); ++ci)
        for (const auto &ch : clips[ci].channels)
            if (static_cast<int64_t>(ch.joint) >= joints)
                Problem(c, rel,
                        fmt::format("clip {} channel targets joint {} >= joint count {}", ci,
                                    ch.joint, joints));
}

} // namespace

int assetc::CheckRuntime(const std::string &dir)
{
    if (!fs::exists(dir) || !fs::is_directory(dir))
    {
        fmtx::Error(fmt::format("output dir does not exist: {}", dir));
        return 1;
    }

    Ctx c;
    c.root = dir;

    // Collect paths first so we can load every manifest before checking refs
    // against it (a .hmat may be walked before assets.hman).
    std::vector<std::pair<std::string, std::string>> mans, mats, meshes, anims; // (abs, rel)
    std::error_code                                  ec;
    for (fs::recursive_directory_iterator it(dir, fs::directory_options::skip_permission_denied, ec),
         end;
         it != end; it.increment(ec))
    {
        if (ec || !it->is_regular_file())
            continue;
        const auto        p   = it->path();
        const std::string rel = fs::relative(p, dir, ec).generic_string();
        const std::string nm  = p.filename().string();
        if (nm.ends_with(".hman"))
            mans.emplace_back(p.string(), rel);
        else if (nm.ends_with(".hmat"))
            mats.emplace_back(p.string(), rel);
        else if (nm.ends_with(".hmesh"))
            meshes.emplace_back(p.string(), rel);
        else if (nm.ends_with(".hanim"))
            anims.emplace_back(p.string(), rel);
    }

    fmt::print("{}Checking {}{}\n", fmtx::BLUE, dir, fmtx::RESET);
    if (mans.empty() && !mats.empty())
        Problem(c, dir, "no assets.hman found but .hmat tables reference textures by hash");

    for (const auto &[p, rel] : mans)
        CheckManifest(c, p, rel);
    for (const auto &[p, rel] : mats)
        CheckHMat(c, p, rel);
    for (const auto &[p, rel] : meshes)
        CheckHMesh(c, p, rel);
    for (const auto &[p, rel] : anims)
        CheckHAnim(c, p, rel);

    if (c.problems == 0)
    {
        fmtx::Success(
            fmt::format("integrity OK: {} manifest(s), {} material table(s), {} mesh(es), {} anim(s)",
                        mans.size(), mats.size(), meshes.size(), anims.size()));
        return 0;
    }
    fmtx::Error(fmt::format("{} integrity problem(s) found", c.problems));
    return 1;
}
