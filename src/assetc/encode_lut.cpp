#include "encode_lut.hpp"

#include "../deps/fmt.hpp"
#include "../deps/ktx.hpp"

#include <cstdint>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

namespace assetc
{

namespace
{

// Quantize a [0,1] float channel to 8-bit UNORM, counting out-of-range clamps.
uint8_t ToU8(float v, uint64_t &clampCount) noexcept
{
    if (v < 0.0f)
    {
        v = 0.0f;
        ++clampCount;
    }
    else if (v > 1.0f)
    {
        v = 1.0f;
        ++clampCount;
    }
    return static_cast<uint8_t>(v * 255.0f + 0.5f);
}

} // namespace

int EncodeLutCube(const std::string &srcPath, const std::string &destPath)
{
    std::ifstream in(srcPath);
    if (!in)
    {
        fmtx::Error(fmt::format("LUT: cannot open {}", srcPath));
        return 1;
    }

    int   size         = 0;
    float domainMin[3] = {0.0f, 0.0f, 0.0f};
    float domainMax[3] = {1.0f, 1.0f, 1.0f};

    std::vector<float> data; // r,g,b triples in red-fastest order (.cube native)

    std::string line;
    int         lineNo = 0;
    while (std::getline(in, line))
    {
        ++lineNo;
        if (!line.empty() && line.back() == '\r') // tolerate CRLF
            line.pop_back();

        const size_t i = line.find_first_not_of(" \t");
        if (i == std::string::npos) // blank
            continue;
        if (line[i] == '#') // comment
            continue;

        const std::string body = line.substr(i);
        std::istringstream ss(body);
        std::string        key;
        ss >> key;

        if (key == "TITLE")
            continue;
        if (key == "LUT_1D_SIZE")
        {
            fmtx::Error(fmt::format(
                "LUT: {} is a 1D LUT; only 3D (.cube LUT_3D_SIZE) is supported", srcPath));
            return 1;
        }
        if (key == "LUT_3D_SIZE")
        {
            if (!(ss >> size) || size < 2 || size > 256)
            {
                fmtx::Error(fmt::format("LUT: {}: bad LUT_3D_SIZE", srcPath));
                return 1;
            }
            data.reserve(static_cast<size_t>(size) * size * size * 3);
            continue;
        }
        if (key == "DOMAIN_MIN")
        {
            ss >> domainMin[0] >> domainMin[1] >> domainMin[2];
            continue;
        }
        if (key == "DOMAIN_MAX")
        {
            ss >> domainMax[0] >> domainMax[1] >> domainMax[2];
            continue;
        }

        // Otherwise: a data row "r g b" (re-parse from the first token).
        std::istringstream ds(body);
        float              r, g, b;
        if (!(ds >> r >> g >> b))
        {
            fmtx::Warn(fmt::format("LUT: {}:{}: skipping unparseable line", srcPath, lineNo));
            continue;
        }
        data.push_back(r);
        data.push_back(g);
        data.push_back(b);
    }

    if (size == 0)
    {
        fmtx::Error(fmt::format("LUT: {}: missing LUT_3D_SIZE", srcPath));
        return 1;
    }

    const size_t voxels = static_cast<size_t>(size) * size * size;
    if (data.size() != voxels * 3)
    {
        fmtx::Error(fmt::format("LUT: {}: expected {} entries, got {}", srcPath, voxels,
                                data.size() / 3));
        return 1;
    }

    // Non-default domain only changes how the shader maps an input color to a
    // sample coordinate; the stored values are unaffected. assetc does not bake
    // the domain in, so flag it for the shader side.
    const bool defaultDomain = domainMin[0] == 0.0f && domainMin[1] == 0.0f &&
                               domainMin[2] == 0.0f && domainMax[0] == 1.0f &&
                               domainMax[1] == 1.0f && domainMax[2] == 1.0f;
    if (!defaultDomain)
        fmtx::Warn(fmt::format(
            "LUT: {}: non-default DOMAIN [{},{},{}]..[{},{},{}]; shader must remap input coords",
            srcPath, domainMin[0], domainMin[1], domainMin[2], domainMax[0], domainMax[1],
            domainMax[2]));

    // .cube is red-fastest, which is exactly the x-fastest memory order of a 3D
    // texture under R->x, G->y, B->z — so no reordering is needed.
    std::vector<uint8_t> rgba(voxels * 4);
    uint64_t             clamped = 0;
    for (size_t e = 0; e < voxels; ++e)
    {
        rgba[e * 4 + 0] = ToU8(data[e * 3 + 0], clamped);
        rgba[e * 4 + 1] = ToU8(data[e * 3 + 1], clamped);
        rgba[e * 4 + 2] = ToU8(data[e * 3 + 2], clamped);
        rgba[e * 4 + 3] = 255;
    }
    if (clamped)
        fmtx::Warn(fmt::format(
            "LUT: {}: clamped {} out-of-[0,1] channel value(s) to fit 8-bit UNORM; "
            "use a float (RGBA16F) LUT for HDR/log data",
            srcPath, clamped));

    const auto err =
        ktx::FromLut3DToKtx2(static_cast<uint32_t>(size), rgba.data(), rgba.size(), destPath);
    if (err != KTX_SUCCESS)
    {
        fmtx::Error(
            fmt::format("LUT: {}: ktx write failed (code {})", destPath, static_cast<int>(err)));
        return 1;
    }

    fmtx::Success(fmt::format("LUT {}^3 -> {}", size, destPath));
    return 0;
}

} // namespace assetc
