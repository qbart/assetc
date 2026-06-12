#pragma once

// Minimal Vulkan + GLFW + Dear ImGui host for the `assetc ui` inspector.
//
// This is deliberately a thin shell over Dear ImGui's own Vulkan helpers
// (ImGui_ImplVulkanH_Window handles the swapchain, framebuffers and per-frame
// sync) plus a small RGBA8 texture uploader so the UI can draw decoded KTX2
// mips with ImGui::Image. No engine/renderer machinery — just enough Vulkan to
// get pixels on screen.

#include <cstdint>
#include <vector>

struct GLFWwindow;

namespace viewer
{

// One uploaded GPU texture. A KTX2 mip chain is uploaded into a single VkImage;
// `mipIds[i]` is an ImGui texture id (a VkDescriptorSet, opaque here) that draws
// mip level i, so the UI can flip levels instantly with a slider.
struct GpuTexture
{
    uint64_t              image  = 0; // VkImage      (opaque to keep vulkan.h out of headers)
    uint64_t              memory = 0; // VkDeviceMemory
    std::vector<uint64_t> mipViews;   // VkImageView per level
    std::vector<void *>   mipIds;     // ImTextureID per level (cast to ImGui's id type)
    uint32_t              width  = 0;
    uint32_t              height = 0;
    uint32_t              mips   = 0;
    bool                  valid  = false;
};

// One mip level's CPU pixels (tightly packed RGBA8, width*height*4 bytes).
struct MipPixels
{
    uint32_t                  width;
    uint32_t                  height;
    std::vector<std::uint8_t> rgba;
};

class GpuContext
{
  public:
    // Create the window, Vulkan device and ImGui context. Returns false (with a
    // logged reason) if no usable Vulkan device/surface is available — the caller
    // should fall back to a CLI message rather than crash.
    bool Init(const char *title, int width, int height);
    void Shutdown();

    // Drive one frame. BeginFrame polls events + starts the ImGui frame and
    // returns false when the window should close. Issue ImGui UI between the two
    // calls; EndFrame renders + presents.
    bool BeginFrame();
    void EndFrame();

    // Upload an RGBA8 mip chain as one texture. `mips[0]` is the base level.
    GpuTexture CreateTexture(const std::vector<MipPixels> &mips);
    // Free a texture's image/memory/views + ImGui descriptor sets. Waits for the
    // device to go idle first (an inspector switches textures rarely).
    void DestroyTexture(GpuTexture &tex);

  private:
    void *win_ = nullptr; // GLFWwindow*
};

} // namespace viewer
