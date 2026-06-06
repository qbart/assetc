#include "runtime_material.hpp"

#include "../deps/fmt.hpp"

#include <bit>
#include <fstream>
#include <ios>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

int assetc::WriteHMat(const std::string &path, std::span<const GpuMaterial> rows)
{
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
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
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}

int assetc::ValidateHMat(const std::string &path)
{
    std::ifstream in(path, std::ios::binary | std::ios::ate);
    if (!in)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }
    const auto size = static_cast<uint64_t>(in.tellg());
    in.seekg(0);

    MatFileHeader hdr{};
    if (size < sizeof(hdr) || !in.read(reinterpret_cast<char *>(&hdr), sizeof(hdr)))
    {
        fmtx::Error(fmt::format("hmat too small: {}", path));
        return 1;
    }
    if (hdr.magic != MatMagic)
    {
        fmtx::Error(fmt::format("hmat bad magic: {}", path));
        return 1;
    }
    if (hdr.version != MatVersion)
    {
        fmtx::Error(fmt::format("hmat bad version {} (want {}): {}", hdr.version, MatVersion, path));
        return 1;
    }
    const uint64_t expect = sizeof(MatFileHeader) +
                            static_cast<uint64_t>(hdr.count) * sizeof(GpuMaterial);
    if (size != expect)
    {
        fmtx::Error(fmt::format("hmat size {} != expected {} ({} rows): {}", size, expect,
                                hdr.count, path));
        return 1;
    }
    return 0;
}
