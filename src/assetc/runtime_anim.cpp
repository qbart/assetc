#include "runtime_anim.hpp"

#include "../deps/fmt.hpp"

#include <bit>
#include <fstream>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{
template <typename T> void Put(std::ofstream &out, const T &v)
{
    out.write(reinterpret_cast<const char *>(&v), sizeof(v));
}
template <typename T> bool Get(std::ifstream &in, T &v)
{
    return static_cast<bool>(in.read(reinterpret_cast<char *>(&v), sizeof(v)));
}
uint8_t Components(assetc::AnimPath p) noexcept
{
    return p == assetc::AnimPath::Rotation ? 4 : 3;
}
} // namespace

int assetc::WriteHAnim(const std::string &path, const std::vector<AnimClip> &clips)
{
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }
    Put(out, AnimMagic);
    Put(out, AnimVersion);
    Put(out, static_cast<uint32_t>(clips.size()));
    Put(out, static_cast<uint32_t>(0));

    for (const auto &c : clips)
    {
        Put(out, static_cast<uint16_t>(c.name.size()));
        out.write(c.name.data(), static_cast<std::streamsize>(c.name.size()));
        Put(out, c.duration);
        Put(out, static_cast<uint32_t>(c.channels.size()));
        for (const auto &ch : c.channels)
        {
            const uint8_t comps = Components(ch.path);
            Put(out, ch.joint);
            Put(out, static_cast<uint8_t>(ch.path));
            Put(out, static_cast<uint8_t>(ch.interp));
            Put(out, comps);
            Put(out, static_cast<uint8_t>(0)); // _pad
            Put(out, static_cast<uint32_t>(ch.keys.size()));
            for (const auto &k : ch.keys)
            {
                Put(out, k.time);
                out.write(reinterpret_cast<const char *>(k.value),
                          static_cast<std::streamsize>(comps) * sizeof(float));
            }
        }
    }
    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ReadHAnim(const std::string &path, std::vector<AnimClip> &out)
{
    out.clear();
    std::ifstream in(path, std::ios::binary);
    uint32_t      magic = 0, version = 0, clipCount = 0, reserved = 0;
    if (!in || !Get(in, magic) || !Get(in, version) || !Get(in, clipCount) || !Get(in, reserved))
    {
        fmtx::Error(fmt::format("hanim too small: {}", path));
        return 1;
    }
    if (magic != AnimMagic)
    {
        fmtx::Error(fmt::format("hanim bad magic: {}", path));
        return 1;
    }
    if (version != AnimVersion)
    {
        fmtx::Error(fmt::format("hanim bad version {} (want {}): {}", version, AnimVersion, path));
        return 1;
    }

    for (uint32_t i = 0; i < clipCount; ++i)
    {
        AnimClip c;
        uint16_t nameLen = 0;
        if (!Get(in, nameLen))
            return 1;
        c.name.resize(nameLen);
        if (nameLen && !in.read(c.name.data(), nameLen))
            return 1;
        uint32_t channelCount = 0;
        if (!Get(in, c.duration) || !Get(in, channelCount))
            return 1;
        for (uint32_t j = 0; j < channelCount; ++j)
        {
            AnimChannel ch;
            uint8_t     path = 0, interp = 0, comps = 0, pad = 0;
            uint32_t    keyCount = 0;
            if (!Get(in, ch.joint) || !Get(in, path) || !Get(in, interp) || !Get(in, comps) ||
                !Get(in, pad) || !Get(in, keyCount))
                return 1;
            ch.path   = static_cast<AnimPath>(path);
            ch.interp = static_cast<AnimInterp>(interp);
            ch.keys.resize(keyCount);
            for (auto &k : ch.keys)
            {
                k.value[0] = k.value[1] = k.value[2] = k.value[3] = 0.0f;
                if (!Get(in, k.time) ||
                    !in.read(reinterpret_cast<char *>(k.value),
                             static_cast<std::streamsize>(comps) * sizeof(float)))
                    return 1;
            }
            c.channels.push_back(std::move(ch));
        }
        out.push_back(std::move(c));
    }
    return 0;
}

int assetc::ValidateHAnim(const std::string &path)
{
    std::vector<AnimClip> clips;
    if (ReadHAnim(path, clips) != 0)
        return 1;
    // Ensure there are no trailing bytes by comparing the consumed size.
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    const auto    fileSize = static_cast<uint64_t>(in.tellg());
    uint64_t      consumed = 16;
    for (const auto &c : clips)
    {
        consumed += 2 + c.name.size() + 4 + 4;
        for (const auto &ch : c.channels)
        {
            const uint64_t comps = ch.path == AnimPath::Rotation ? 4 : 3;
            consumed += 4 + 1 + 1 + 1 + 1 + 4;
            consumed += ch.keys.size() * (4 + comps * 4);
        }
    }
    if (consumed != fileSize)
    {
        fmtx::Error(fmt::format("hanim size {} != parsed {}: {}", fileSize, consumed, path));
        return 1;
    }
    return 0;
}
