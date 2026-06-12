#include "assetc/runtime_material.hpp"

#include "diag.hpp"

#include <bit>
#include <format>
#include <fstream>
#include <ios>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

int assetc::WriteHMat(const std::string &path, std::span<const GpuMaterial> rows)
{
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        assetc::diag::Error(std::format("open failed: {}", path));
        return 1;
    }

    MatFileHeader hdr{};
    hdr.magic   = MatMagic;
    hdr.version = MatVersion;
    hdr.count   = static_cast<uint32_t>(rows.size());
    hdr.flags   = 0;
    out.write(reinterpret_cast<const char *>(&hdr), sizeof(hdr));
    out.write(reinterpret_cast<const char *>(rows.data()),
              static_cast<std::streamsize>(rows.size() * sizeof(GpuMaterial)));

    if (!out.good())
    {
        assetc::diag::Error(std::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ValidateHMat(const std::string &path)
{
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        assetc::diag::Error(std::format("open failed: {}", path));
        return 1;
    }
    const auto size = static_cast<uint64_t>(in.tellg());
    in.seekg(0);

    MatFileHeader hdr{};
    if (size < sizeof(hdr) || !in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
    {
        assetc::diag::Error(std::format("hmat too small: {}", path));
        return 1;
    }
    if (hdr.magic != MatMagic)
    {
        assetc::diag::Error(std::format("hmat bad magic: {}", path));
        return 1;
    }
    if (hdr.version != MatVersion)
    {
        assetc::diag::Error(std::format("hmat bad version {} (want {}): {}", hdr.version, MatVersion, path));
        return 1;
    }
    const uint64_t expect = sizeof(MatFileHeader) +
                            static_cast<uint64_t>(hdr.count) * sizeof(GpuMaterial);
    if (size != expect)
    {
        assetc::diag::Error(std::format("hmat size {} != expected {} ({} rows): {}", size, expect,
                                hdr.count, path));
        return 1;
    }
    return 0;
}
