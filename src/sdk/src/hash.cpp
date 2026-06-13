#include "assetc/hash.hpp"

#include <cstdint>

namespace
{
constexpr uint64_t FnvOffset = 0xcbf29ce484222325ULL;
constexpr uint64_t FnvPrime  = 0x00000100000001b3ULL;

constexpr char AsciiLower(char c) noexcept
{
    return (c >= 'A' && c <= 'Z') ? static_cast<char>(c - 'A' + 'a') : c;
}

constexpr uint64_t Fnv1aLower(std::string_view s) noexcept
{
    uint64_t h = FnvOffset;
    for (char c : s)
    {
        h ^= static_cast<uint8_t>(AsciiLower(c));
        h *= FnvPrime;
    }
    return h;
}
} // namespace

uint64_t assetc::HashAssetRef(std::string_view runtimeRefNoExt) noexcept
{
    return Fnv1aLower(runtimeRefNoExt);
}

uint64_t assetc::HashEmbedRef(std::string_view runtimeRefWithExt) noexcept
{
    // Identical hashing to HashAssetRef; the only difference is the caller keeps the
    // extension in the ref string, so the extension contributes to the id.
    return Fnv1aLower(runtimeRefWithExt);
}
