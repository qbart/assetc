#include "runtime_mesh.hpp"

#include "../deps/fmt.hpp"

#include <bit>
#include <fstream>
#include <ios>

static_assert(std::endian::native == std::endian::little,
              "assetc currently supports little-endian targets only");

namespace
{
constexpr size_t ChunkAlign = 16;

constexpr size_t AlignUp(size_t v, size_t a) noexcept
{
    return (v + a - 1) & ~(a - 1);
}
} // namespace

int assetc::WriteChunked(const std::string &path, std::span<const ChunkPayload> chunks)
{
    // Pass 1: lay out the file. Header, table, then payloads 16-byte aligned.
    std::vector<ChunkEntry> table(chunks.size());
    size_t cursor = sizeof(FileHeader) + chunks.size() * sizeof(ChunkEntry);
    for (size_t i = 0; i < chunks.size(); ++i)
    {
        cursor          = AlignUp(cursor, ChunkAlign);
        table[i].fourcc = std::to_underlying(chunks[i].id);
        table[i].flags  = 0;
        table[i].offset = cursor;
        table[i].size   = chunks[i].bytes.size();
        cursor += chunks[i].bytes.size();
    }

    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    if (!out)
    {
        fmtx::Error(fmt::format("open failed: {}", path));
        return 1;
    }

    FileHeader hdr{};
    hdr.magic      = MeshMagic;
    hdr.version    = 1;
    hdr.chunkCount = static_cast<uint32_t>(chunks.size());
    out.write(reinterpret_cast<const char *>(&hdr), sizeof(hdr));
    out.write(reinterpret_cast<const char *>(table.data()),
              static_cast<std::streamsize>(table.size() * sizeof(ChunkEntry)));

    static constexpr uint8_t padBytes[ChunkAlign]{};
    size_t                   written = sizeof(FileHeader) + table.size() * sizeof(ChunkEntry);
    for (size_t i = 0; i < chunks.size(); ++i)
    {
        size_t pad = table[i].offset - written;
        if (pad > 0)
            out.write(reinterpret_cast<const char *>(padBytes), static_cast<std::streamsize>(pad));
        out.write(reinterpret_cast<const char *>(chunks[i].bytes.data()),
                  static_cast<std::streamsize>(chunks[i].bytes.size()));
        written = table[i].offset + chunks[i].bytes.size();
    }

    if (!out.good())
    {
        fmtx::Error(fmt::format("write failed: {}", path));
        return 1;
    }
    return 0;
}
