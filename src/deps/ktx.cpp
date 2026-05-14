#include "ktx.hpp"

#include <bit>
#include <cstring>
#include <fmt/core.h>
#include <fmt/format.h>
#include <memory>
#include <vector>
#include <vulkan/vulkan.hpp>
#include <thread>

namespace
{
struct KtxTexture2Deleter
{
    void operator()(ktxTexture2 *t) const noexcept
    {
        if (t) ktxTexture_Destroy(ktxTexture(t));
    }
};
using KtxTexture2Ptr = std::unique_ptr<ktxTexture2, KtxTexture2Deleter>;
} // namespace

KTX_error_code ktx::FromImageToASTC(const stb::Image &img, const std::string &destPath, ASTCMode mode)
{
    // auto threadCount = 1u; // std::thread::hardware_concurrency();
    auto threadCount = std::thread::hardware_concurrency();

    // Color: sRGB OETF in the output DFD (--assign_oetf srgb, the toktx default for 8-bit PNGs).
    // Normal/Grayscale: linear OETF (--assign_oetf linear).
    const VkFormat srcFormat =
        (mode == ASTCMode::Color) ? VK_FORMAT_R8G8B8A8_SRGB : VK_FORMAT_R8G8B8A8_UNORM;

    ktxTextureCreateInfo info{};
    info.vkFormat        = srcFormat;
    info.baseWidth       = img.w;
    info.baseHeight      = img.h;
    info.baseDepth       = 1;
    info.numDimensions   = 2;
    info.numLevels       = std::bit_width(static_cast<unsigned>(std::max(img.w, img.h)));
    info.numLayers       = 1;
    info.numFaces        = 1;
    info.generateMipmaps = KTX_FALSE; // libktx ignores this; you must fill levels

    ktxTexture2 *tex = nullptr;
    auto err         = ktxTexture2_Create(&info, KTX_TEXTURE_CREATE_ALLOC_STORAGE, &tex);
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    // level 0
    err = ktxTexture_SetImageFromMemory(ktxTexture(tex), 0, 0, 0, img.pixels, img.Size());
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    // levels 1..N — downscale yourself, then upload
    for (uint32_t lvl = 1; lvl < info.numLevels; ++lvl)
    {
        int lw   = std::max(1, img.w >> lvl);
        int lh   = std::max(1, img.h >> lvl);
        auto mip = stb::Resize(img, lw, lh);

        err = ktxTexture_SetImageFromMemory(ktxTexture(tex), lvl, 0, 0, mip->pixels, mip->Size());
        if (err != KTX_SUCCESS)
        {
            return err;
        }
    }

    // ASTC encode = --encode astc --astc_blk_d 4x4 --astc_quality medium
    ktxAstcParams astc{};
    astc.structSize     = sizeof(astc);
    astc.threadCount    = threadCount;
    astc.blockDimension = KTX_PACK_ASTC_BLOCK_DIMENSION_4x4;
    astc.qualityLevel   = KTX_PACK_ASTC_QUALITY_LEVEL_THOROUGH;

    switch (mode)
    {
    case ASTCMode::Color:
        break;
    case ASTCMode::Normal:
        // Encode as 2-channel L+A with angular error metric.
        // Encoder applies its own "rrrg" swizzle internally; X ends up in .a, Y in .g.
        //
        // Shader sampling (GLSL):
        //   vec3 n;
        //   n.xy = texture(normalMap, uv).ag * 2.0 - 1.0;   // .ag, not .rg
        //   n.z  = sqrt(max(0.0, 1.0 - dot(n.xy, n.xy)));
        //
        // Old encoding was: std::memcpy(astc.inputSwizzle, "rg01", 4);
        // matching shader sampling (GLSL):
        //   vec3 n;
        //   n.xy = texture(normalMap, uv).rg * 2.0 - 1.0;
        //   n.z  = sqrt(max(0.0, 1.0 - dot(n.xy, n.xy)));
        astc.normalMap = KTX_TRUE;
        break;
    case ASTCMode::Grayscale:
        std::memcpy(astc.inputSwizzle, "r001", 4);
        break;
    }

    err = ktxTexture2_CompressAstcEx(tex, &astc);
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    err = ktxTexture_WriteToNamedFile(ktxTexture(tex), destPath.c_str());
    if (err != KTX_SUCCESS)
    {
        return err;
    }
    ktxTexture_Destroy(ktxTexture(tex));

    return KTX_SUCCESS;
}

// toktx mapping:
//   Color     -> toktx --t2 --genmipmap --encode uastc --uastc_quality 3 <dst> <src>
//   Normal    -> toktx --t2 --genmipmap --encode uastc --uastc_quality 3 --assign_oetf linear --input_swizzle rg01 <dst> <src>
//   Grayscale -> toktx --t2 --genmipmap --encode uastc --uastc_quality 3 --assign_oetf linear --input_swizzle r001 <dst> <src>
//
// UASTC = Basis Universal high-quality mode. Output KTX2 transcodes at runtime
// to BC7/ASTC/ETC2 with quality close to native BC7. Larger than ETC1S, much
// better quality. Single file works across desktop + mobile.
KTX_error_code ktx::FromImageToUASTC(const stb::Image &img, const std::string &destPath, UASTCMode mode)
{
    // Outer pool (in main.cpp) parallelizes across assets, so encode single-threaded here
    // to avoid oversubscription. Flip to hardware_concurrency() if you switch to a
    // sequential outer loop with one large texture at a time.
    auto threadCount = 1u;

    const VkFormat srcFormat =
        (mode == UASTCMode::Color) ? VK_FORMAT_R8G8B8A8_SRGB : VK_FORMAT_R8G8B8A8_UNORM;

    ktxTextureCreateInfo info{};
    info.vkFormat        = srcFormat;
    info.baseWidth       = img.w;
    info.baseHeight      = img.h;
    info.baseDepth       = 1;
    info.numDimensions   = 2;
    info.numLevels       = std::bit_width(static_cast<unsigned>(std::max(img.w, img.h)));
    info.numLayers       = 1;
    info.numFaces        = 1;
    info.generateMipmaps = KTX_FALSE;

    ktxTexture2 *raw = nullptr;
    auto err         = ktxTexture2_Create(&info, KTX_TEXTURE_CREATE_ALLOC_STORAGE, &raw);
    if (err != KTX_SUCCESS)
    {
        return err;
    }
    KtxTexture2Ptr tex(raw); // takes ownership; destroyed on every return path below

    err = ktxTexture_SetImageFromMemory(ktxTexture(tex.get()), 0, 0, 0, img.pixels, img.Size());
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    for (uint32_t lvl = 1; lvl < info.numLevels; ++lvl)
    {
        int lw   = std::max(1, img.w >> lvl);
        int lh   = std::max(1, img.h >> lvl);
        auto mip = stb::Resize(img, lw, lh);

        err = ktxTexture_SetImageFromMemory(ktxTexture(tex.get()), lvl, 0, 0, mip->pixels, mip->Size());
        if (err != KTX_SUCCESS)
        {
            return err;
        }
    }

    ktxBasisParams basis{};
    basis.structSize  = sizeof(basis);
    basis.uastc       = KTX_TRUE; // --encode uastc
    basis.threadCount = threadCount;
    // --uastc_quality 3 (SLOWER): 0=fastest .. 4=very-slow/highest
    basis.uastcFlags = KTX_PACK_UASTC_LEVEL_SLOWER;

    switch (mode)
    {
    case UASTCMode::Color:
        break;
    case UASTCMode::Normal:
        // Shader sampling (GLSL):
        //   vec3 n;
        //   n.xy = texture(normalMap, uv).rg * 2.0 - 1.0;
        //   n.z  = sqrt(max(0.0, 1.0 - dot(n.xy, n.xy)));
        std::memcpy(basis.inputSwizzle, "rg01", 4);
        break;
    case UASTCMode::Grayscale:
        std::memcpy(basis.inputSwizzle, "r001", 4);
        break;
    }

    err = ktxTexture2_CompressBasisEx(tex.get(), &basis);
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    err = ktxTexture_WriteToNamedFile(ktxTexture(tex.get()), destPath.c_str());
    if (err != KTX_SUCCESS)
    {
        return err;
    }

    return KTX_SUCCESS;
}
