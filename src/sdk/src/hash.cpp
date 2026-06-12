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
} // namespace

uint64_t assetc::HashAssetRef(std::string_view runtimeRefNoExt) noexcept
{
    uint64_t h = FnvOffset;
    for (char c : runtimeRefNoExt)
    {
        const auto b = static_cast<uint8_t>(AsciiLower(c));
        h ^= b;
        h *= FnvPrime;
    }
    return h;
}
