#include "ktx.hpp"

#include <bit>
#include <fmt/core.h>
#include <fmt/format.h>
#include <vector>
#include <vulkan/vulkan.hpp>

KTX_error_code ktx::FromImageToASTC(const stb::Image &img, const std::string &destPath)
{
    auto threadCount = 1u; // std::thread::hardware_concurrency();

    ktxTextureCreateInfo info{};
    info.vkFormat        = VK_FORMAT_R8G8B8A8_UNORM; // source format
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

    // ASTC encode = your --encode astc --astc_blk_d 4x4 --astc_quality medium
    ktxAstcParams astc{};
    astc.structSize     = sizeof(astc);
    astc.threadCount    = threadCount;
    astc.blockDimension = KTX_PACK_ASTC_BLOCK_DIMENSION_4x4;
    astc.qualityLevel   = KTX_PACK_ASTC_QUALITY_LEVEL_MEDIUM;
    // astc.normalMap   = KTX_TRUE;  // for normal maps

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
