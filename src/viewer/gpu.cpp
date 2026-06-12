#include "gpu.hpp"

#include "deps/fmt.hpp"

#include <cstring>
#include <vector>

#define GLFW_INCLUDE_VULKAN
#include <GLFW/glfw3.h>

#include <imgui.h>
#include <imgui_impl_glfw.h>
#include <imgui_impl_vulkan.h>

// ---------------------------------------------------------------------------
// All Vulkan state lives in this TU at file scope: the viewer is a single
// window, single-threaded app, so there is nothing to gain from threading the
// handles through GpuContext. The class is just the public lifecycle + window.
// The frame loop and Vulkan setup mirror Dear ImGui's example_glfw_vulkan.
// ---------------------------------------------------------------------------

namespace
{
VkAllocationCallbacks   *g_Allocator      = nullptr;
VkInstance               g_Instance       = VK_NULL_HANDLE;
VkPhysicalDevice         g_PhysicalDevice = VK_NULL_HANDLE;
VkDevice                 g_Device         = VK_NULL_HANDLE;
uint32_t                 g_QueueFamily    = (uint32_t)-1;
VkQueue                  g_Queue          = VK_NULL_HANDLE;
VkDescriptorPool         g_DescriptorPool = VK_NULL_HANDLE;
ImGui_ImplVulkanH_Window g_MainWindowData;
int                      g_MinImageCount    = 2;
bool                     g_SwapChainRebuild = false;

VkCommandPool                    g_UploadPool = VK_NULL_HANDLE; // transient, for texture uploads
VkSampler                        g_Sampler    = VK_NULL_HANDLE; // shared NEAREST sampler
VkPhysicalDeviceMemoryProperties g_MemProps{};

void check_vk(VkResult err)
{
    if (err != VK_SUCCESS)
        fmtx::Error(fmt::format("vulkan error: {}", (int)err));
}

// Prefer a discrete GPU, else the first device. (ImGui 1.91.5 dropped the
// ImGui_ImplVulkanH_Select* helpers, so we do this ourselves.)
VkPhysicalDevice PickPhysicalDevice(VkInstance inst)
{
    uint32_t n = 0;
    vkEnumeratePhysicalDevices(inst, &n, nullptr);
    if (n == 0)
        return VK_NULL_HANDLE;
    std::vector<VkPhysicalDevice> devs(n);
    vkEnumeratePhysicalDevices(inst, &n, devs.data());
    for (VkPhysicalDevice d : devs)
    {
        VkPhysicalDeviceProperties p{};
        vkGetPhysicalDeviceProperties(d, &p);
        if (p.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU)
            return d;
    }
    return devs[0];
}

uint32_t PickGraphicsQueueFamily(VkPhysicalDevice dev)
{
    uint32_t n = 0;
    vkGetPhysicalDeviceQueueFamilyProperties(dev, &n, nullptr);
    std::vector<VkQueueFamilyProperties> q(n);
    vkGetPhysicalDeviceQueueFamilyProperties(dev, &n, q.data());
    for (uint32_t i = 0; i < n; ++i)
        if (q[i].queueFlags & VK_QUEUE_GRAPHICS_BIT)
            return i;
    return (uint32_t)-1;
}

uint32_t FindMemoryType(uint32_t typeBits, VkMemoryPropertyFlags props)
{
    for (uint32_t i = 0; i < g_MemProps.memoryTypeCount; ++i)
        if ((typeBits & (1u << i)) &&
            (g_MemProps.memoryTypes[i].propertyFlags & props) == props)
            return i;
    return (uint32_t)-1;
}

bool SetupVulkan(const char **exts, uint32_t extCount)
{
    VkApplicationInfo app{VK_STRUCTURE_TYPE_APPLICATION_INFO};
    app.pApplicationName = "assetc ui";
    app.apiVersion       = VK_API_VERSION_1_1;

    VkInstanceCreateInfo ci{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO};
    ci.pApplicationInfo        = &app;
    ci.enabledExtensionCount   = extCount;
    ci.ppEnabledExtensionNames = exts;
    if (vkCreateInstance(&ci, g_Allocator, &g_Instance) != VK_SUCCESS)
    {
        fmtx::Error("vkCreateInstance failed");
        return false;
    }

    g_PhysicalDevice = PickPhysicalDevice(g_Instance);
    if (g_PhysicalDevice == VK_NULL_HANDLE)
    {
        fmtx::Error("no Vulkan physical device found");
        return false;
    }
    vkGetPhysicalDeviceMemoryProperties(g_PhysicalDevice, &g_MemProps);
    g_QueueFamily = PickGraphicsQueueFamily(g_PhysicalDevice);
    if (g_QueueFamily == (uint32_t)-1)
    {
        fmtx::Error("no graphics queue family found");
        return false;
    }

    const char     *devExt[]  = {VK_KHR_SWAPCHAIN_EXTENSION_NAME};
    const float     prio       = 1.0f;
    VkDeviceQueueCreateInfo q{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
    q.queueFamilyIndex = g_QueueFamily;
    q.queueCount       = 1;
    q.pQueuePriorities = &prio;
    VkDeviceCreateInfo dci{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO};
    dci.queueCreateInfoCount    = 1;
    dci.pQueueCreateInfos       = &q;
    dci.enabledExtensionCount   = 1;
    dci.ppEnabledExtensionNames = devExt;
    if (vkCreateDevice(g_PhysicalDevice, &dci, g_Allocator, &g_Device) != VK_SUCCESS)
    {
        fmtx::Error("vkCreateDevice failed");
        return false;
    }
    vkGetDeviceQueue(g_Device, g_QueueFamily, 0, &g_Queue);

    // Generous pool: ImGui's own font set + one combined-image-sampler per mip of
    // whatever texture is on screen. FREE bit lets us reclaim on texture switch.
    VkDescriptorPoolSize sizes[] = {{VK_DESCRIPTOR_TYPE_COMBINED_IMAGE_SAMPLER, 1024}};
    VkDescriptorPoolCreateInfo dp{VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO};
    dp.flags         = VK_DESCRIPTOR_POOL_CREATE_FREE_DESCRIPTOR_SET_BIT;
    dp.maxSets       = 1024;
    dp.poolSizeCount = 1;
    dp.pPoolSizes    = sizes;
    check_vk(vkCreateDescriptorPool(g_Device, &dp, g_Allocator, &g_DescriptorPool));

    VkCommandPoolCreateInfo cp{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
    cp.flags            = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT;
    cp.queueFamilyIndex = g_QueueFamily;
    check_vk(vkCreateCommandPool(g_Device, &cp, g_Allocator, &g_UploadPool));

    VkSamplerCreateInfo sm{VK_STRUCTURE_TYPE_SAMPLER_CREATE_INFO};
    sm.magFilter    = VK_FILTER_NEAREST; // inspector: show actual texels when zoomed
    sm.minFilter    = VK_FILTER_LINEAR;
    sm.mipmapMode   = VK_SAMPLER_MIPMAP_MODE_NEAREST;
    sm.addressModeU = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE;
    sm.addressModeV = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE;
    sm.addressModeW = VK_SAMPLER_ADDRESS_MODE_CLAMP_TO_EDGE;
    sm.maxLod       = VK_LOD_CLAMP_NONE;
    check_vk(vkCreateSampler(g_Device, &sm, g_Allocator, &g_Sampler));
    return true;
}

void SetupVulkanWindow(ImGui_ImplVulkanH_Window *wd, VkSurfaceKHR surface, int w, int h)
{
    wd->Surface = surface;

    VkBool32 supported = VK_FALSE;
    vkGetPhysicalDeviceSurfaceSupportKHR(g_PhysicalDevice, g_QueueFamily, wd->Surface, &supported);
    if (!supported)
        fmtx::Error("selected queue family cannot present to the window surface");

    const VkFormat fmts[] = {VK_FORMAT_B8G8R8A8_UNORM, VK_FORMAT_R8G8B8A8_UNORM,
                             VK_FORMAT_B8G8R8_UNORM, VK_FORMAT_R8G8B8_UNORM};
    const VkColorSpaceKHR cs = VK_COLOR_SPACE_SRGB_NONLINEAR_KHR;
    wd->SurfaceFormat        = ImGui_ImplVulkanH_SelectSurfaceFormat(
        g_PhysicalDevice, wd->Surface, fmts, (size_t)IM_ARRAYSIZE(fmts), cs);

    VkPresentModeKHR present[] = {VK_PRESENT_MODE_FIFO_KHR};
    wd->PresentMode            = ImGui_ImplVulkanH_SelectPresentMode(
        g_PhysicalDevice, wd->Surface, &present[0], IM_ARRAYSIZE(present));

    ImGui_ImplVulkanH_CreateOrResizeWindow(g_Instance, g_PhysicalDevice, g_Device, wd,
                                           g_QueueFamily, g_Allocator, w, h, g_MinImageCount);
}

void FrameRender(ImGui_ImplVulkanH_Window *wd, ImDrawData *draw_data)
{
    VkSemaphore img_acq = wd->FrameSemaphores[wd->SemaphoreIndex].ImageAcquiredSemaphore;
    VkSemaphore render_done = wd->FrameSemaphores[wd->SemaphoreIndex].RenderCompleteSemaphore;
    VkResult err = vkAcquireNextImageKHR(g_Device, wd->Swapchain, UINT64_MAX, img_acq,
                                         VK_NULL_HANDLE, &wd->FrameIndex);
    if (err == VK_ERROR_OUT_OF_DATE_KHR || err == VK_SUBOPTIMAL_KHR)
    {
        g_SwapChainRebuild = true;
        if (err == VK_ERROR_OUT_OF_DATE_KHR)
            return;
    }

    ImGui_ImplVulkanH_Frame *fd = &wd->Frames[wd->FrameIndex];
    vkWaitForFences(g_Device, 1, &fd->Fence, VK_TRUE, UINT64_MAX);
    vkResetFences(g_Device, 1, &fd->Fence);
    vkResetCommandPool(g_Device, fd->CommandPool, 0);

    VkCommandBufferBeginInfo bi{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
    bi.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
    vkBeginCommandBuffer(fd->CommandBuffer, &bi);

    VkRenderPassBeginInfo rp{VK_STRUCTURE_TYPE_RENDER_PASS_BEGIN_INFO};
    rp.renderPass               = wd->RenderPass;
    rp.framebuffer              = fd->Framebuffer;
    rp.renderArea.extent.width  = wd->Width;
    rp.renderArea.extent.height = wd->Height;
    rp.clearValueCount          = 1;
    rp.pClearValues             = &wd->ClearValue;
    vkCmdBeginRenderPass(fd->CommandBuffer, &rp, VK_SUBPASS_CONTENTS_INLINE);

    ImGui_ImplVulkan_RenderDrawData(draw_data, fd->CommandBuffer);

    vkCmdEndRenderPass(fd->CommandBuffer);

    VkPipelineStageFlags wait_stage = VK_PIPELINE_STAGE_COLOR_ATTACHMENT_OUTPUT_BIT;
    VkSubmitInfo si{VK_STRUCTURE_TYPE_SUBMIT_INFO};
    si.waitSemaphoreCount   = 1;
    si.pWaitSemaphores      = &img_acq;
    si.pWaitDstStageMask    = &wait_stage;
    si.commandBufferCount   = 1;
    si.pCommandBuffers      = &fd->CommandBuffer;
    si.signalSemaphoreCount = 1;
    si.pSignalSemaphores    = &render_done;
    vkEndCommandBuffer(fd->CommandBuffer);
    check_vk(vkQueueSubmit(g_Queue, 1, &si, fd->Fence));
}

void FramePresent(ImGui_ImplVulkanH_Window *wd)
{
    if (g_SwapChainRebuild)
        return;
    VkSemaphore render_done = wd->FrameSemaphores[wd->SemaphoreIndex].RenderCompleteSemaphore;
    VkPresentInfoKHR pi{VK_STRUCTURE_TYPE_PRESENT_INFO_KHR};
    pi.waitSemaphoreCount = 1;
    pi.pWaitSemaphores    = &render_done;
    pi.swapchainCount     = 1;
    pi.pSwapchains        = &wd->Swapchain;
    pi.pImageIndices      = &wd->FrameIndex;
    VkResult err          = vkQueuePresentKHR(g_Queue, &pi);
    if (err == VK_ERROR_OUT_OF_DATE_KHR || err == VK_SUBOPTIMAL_KHR)
        g_SwapChainRebuild = true;
    wd->SemaphoreIndex = (wd->SemaphoreIndex + 1) % wd->SemaphoreCount;
}

void glfw_error(int code, const char *desc)
{
    fmtx::Error(fmt::format("glfw error {}: {}", code, desc));
}
} // namespace

namespace viewer
{

bool GpuContext::Init(const char *title, int width, int height)
{
    glfwSetErrorCallback(glfw_error);
    if (!glfwInit())
    {
        fmtx::Error("glfwInit failed (no display? set DISPLAY/WAYLAND_DISPLAY)");
        return false;
    }
    if (!glfwVulkanSupported())
    {
        fmtx::Error("GLFW reports Vulkan is not supported on this system");
        glfwTerminate();
        return false;
    }

    glfwWindowHint(GLFW_CLIENT_API, GLFW_NO_API);
    GLFWwindow *window = glfwCreateWindow(width, height, title, nullptr, nullptr);
    if (!window)
    {
        fmtx::Error("glfwCreateWindow failed");
        glfwTerminate();
        return false;
    }
    win_ = window;

    uint32_t     extCount = 0;
    const char **exts     = glfwGetRequiredInstanceExtensions(&extCount);
    if (!SetupVulkan(exts, extCount))
        return false;

    VkSurfaceKHR surface = VK_NULL_HANDLE;
    if (glfwCreateWindowSurface(g_Instance, window, g_Allocator, &surface) != VK_SUCCESS)
    {
        fmtx::Error("glfwCreateWindowSurface failed");
        return false;
    }

    int fbw = 0, fbh = 0;
    glfwGetFramebufferSize(window, &fbw, &fbh);
    SetupVulkanWindow(&g_MainWindowData, surface, fbw, fbh);

    IMGUI_CHECKVERSION();
    ImGui::CreateContext();
    ImGuiIO &io = ImGui::GetIO();
    io.ConfigFlags |= ImGuiConfigFlags_DockingEnable;
    io.IniFilename = nullptr; // don't litter the cwd with imgui.ini
    ImGui::StyleColorsDark();

    ImGui_ImplGlfw_InitForVulkan(window, true);
    ImGui_ImplVulkan_InitInfo init{};
    init.Instance        = g_Instance;
    init.PhysicalDevice  = g_PhysicalDevice;
    init.Device          = g_Device;
    init.QueueFamily     = g_QueueFamily;
    init.Queue           = g_Queue;
    init.DescriptorPool  = g_DescriptorPool;
    init.RenderPass      = g_MainWindowData.RenderPass;
    init.MinImageCount   = g_MinImageCount;
    init.ImageCount      = g_MainWindowData.ImageCount;
    init.MSAASamples     = VK_SAMPLE_COUNT_1_BIT;
    init.CheckVkResultFn = check_vk;
    ImGui_ImplVulkan_Init(&init);
    return true;
}

bool GpuContext::BeginFrame()
{
    GLFWwindow *window = (GLFWwindow *)win_;
    if (glfwWindowShouldClose(window))
        return false;
    glfwPollEvents();

    if (g_SwapChainRebuild)
    {
        int w = 0, h = 0;
        glfwGetFramebufferSize(window, &w, &h);
        if (w > 0 && h > 0)
        {
            ImGui_ImplVulkan_SetMinImageCount(g_MinImageCount);
            ImGui_ImplVulkanH_CreateOrResizeWindow(g_Instance, g_PhysicalDevice, g_Device,
                                                   &g_MainWindowData, g_QueueFamily, g_Allocator,
                                                   w, h, g_MinImageCount);
            g_MainWindowData.FrameIndex = 0;
            g_SwapChainRebuild          = false;
        }
    }

    ImGui_ImplVulkan_NewFrame();
    ImGui_ImplGlfw_NewFrame();
    ImGui::NewFrame();
    return true;
}

void GpuContext::EndFrame()
{
    ImGui::Render();
    ImDrawData *draw = ImGui::GetDrawData();
    const bool  minimized = draw->DisplaySize.x <= 0.0f || draw->DisplaySize.y <= 0.0f;
    if (!minimized)
    {
        g_MainWindowData.ClearValue.color.float32[0] = 0.10f;
        g_MainWindowData.ClearValue.color.float32[1] = 0.10f;
        g_MainWindowData.ClearValue.color.float32[2] = 0.12f;
        g_MainWindowData.ClearValue.color.float32[3] = 1.00f;
        FrameRender(&g_MainWindowData, draw);
        FramePresent(&g_MainWindowData);
    }
}

GpuTexture GpuContext::CreateTexture(const std::vector<MipPixels> &mips)
{
    GpuTexture tex;
    if (mips.empty() || mips[0].width == 0 || mips[0].height == 0)
        return tex;
    const uint32_t levels = (uint32_t)mips.size();
    tex.width  = mips[0].width;
    tex.height = mips[0].height;
    tex.mips   = levels;

    VkImageCreateInfo ic{VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO};
    ic.imageType     = VK_IMAGE_TYPE_2D;
    ic.format        = VK_FORMAT_R8G8B8A8_UNORM;
    ic.extent        = {tex.width, tex.height, 1};
    ic.mipLevels     = levels;
    ic.arrayLayers   = 1;
    ic.samples       = VK_SAMPLE_COUNT_1_BIT;
    ic.tiling        = VK_IMAGE_TILING_OPTIMAL;
    ic.usage         = VK_IMAGE_USAGE_TRANSFER_DST_BIT | VK_IMAGE_USAGE_SAMPLED_BIT;
    ic.sharingMode   = VK_SHARING_MODE_EXCLUSIVE;
    ic.initialLayout = VK_IMAGE_LAYOUT_UNDEFINED;
    VkImage image = VK_NULL_HANDLE;
    if (vkCreateImage(g_Device, &ic, g_Allocator, &image) != VK_SUCCESS)
        return tex;

    VkMemoryRequirements req{};
    vkGetImageMemoryRequirements(g_Device, image, &req);
    VkMemoryAllocateInfo ai{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    ai.allocationSize  = req.size;
    ai.memoryTypeIndex = FindMemoryType(req.memoryTypeBits, VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT);
    VkDeviceMemory imgMem = VK_NULL_HANDLE;
    check_vk(vkAllocateMemory(g_Device, &ai, g_Allocator, &imgMem));
    vkBindImageMemory(g_Device, image, imgMem, 0);

    // One staging buffer holding all mips back to back.
    VkDeviceSize total = 0;
    for (const auto &m : mips)
        total += (VkDeviceSize)m.rgba.size();
    VkBufferCreateInfo bc{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
    bc.size        = total;
    bc.usage       = VK_BUFFER_USAGE_TRANSFER_SRC_BIT;
    bc.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
    VkBuffer staging = VK_NULL_HANDLE;
    check_vk(vkCreateBuffer(g_Device, &bc, g_Allocator, &staging));
    VkMemoryRequirements sreq{};
    vkGetBufferMemoryRequirements(g_Device, staging, &sreq);
    VkMemoryAllocateInfo sai{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
    sai.allocationSize  = sreq.size;
    sai.memoryTypeIndex = FindMemoryType(sreq.memoryTypeBits,
                                         VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT |
                                             VK_MEMORY_PROPERTY_HOST_COHERENT_BIT);
    VkDeviceMemory stagingMem = VK_NULL_HANDLE;
    check_vk(vkAllocateMemory(g_Device, &sai, g_Allocator, &stagingMem));
    vkBindBufferMemory(g_Device, staging, stagingMem, 0);

    void *mapped = nullptr;
    vkMapMemory(g_Device, stagingMem, 0, total, 0, &mapped);
    std::vector<VkDeviceSize> offsets(levels);
    VkDeviceSize              cursor = 0;
    for (uint32_t i = 0; i < levels; ++i)
    {
        offsets[i] = cursor;
        std::memcpy((uint8_t *)mapped + cursor, mips[i].rgba.data(), mips[i].rgba.size());
        cursor += mips[i].rgba.size();
    }
    vkUnmapMemory(g_Device, stagingMem);

    // One-shot command buffer: transition all levels, copy each, transition to read.
    VkCommandBufferAllocateInfo cbi{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
    cbi.commandPool        = g_UploadPool;
    cbi.level              = VK_COMMAND_BUFFER_LEVEL_PRIMARY;
    cbi.commandBufferCount = 1;
    VkCommandBuffer cmd = VK_NULL_HANDLE;
    vkAllocateCommandBuffers(g_Device, &cbi, &cmd);
    VkCommandBufferBeginInfo cb{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO};
    cb.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
    vkBeginCommandBuffer(cmd, &cb);

    VkImageMemoryBarrier toDst{VK_STRUCTURE_TYPE_IMAGE_MEMORY_BARRIER};
    toDst.oldLayout                   = VK_IMAGE_LAYOUT_UNDEFINED;
    toDst.newLayout                   = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
    toDst.srcQueueFamilyIndex         = VK_QUEUE_FAMILY_IGNORED;
    toDst.dstQueueFamilyIndex         = VK_QUEUE_FAMILY_IGNORED;
    toDst.image                       = image;
    toDst.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
    toDst.subresourceRange.levelCount = levels;
    toDst.subresourceRange.layerCount = 1;
    toDst.srcAccessMask               = 0;
    toDst.dstAccessMask               = VK_ACCESS_TRANSFER_WRITE_BIT;
    vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TOP_OF_PIPE_BIT, VK_PIPELINE_STAGE_TRANSFER_BIT, 0,
                         0, nullptr, 0, nullptr, 1, &toDst);

    for (uint32_t i = 0; i < levels; ++i)
    {
        VkBufferImageCopy region{};
        region.bufferOffset                = offsets[i];
        region.imageSubresource.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        region.imageSubresource.mipLevel   = i;
        region.imageSubresource.layerCount = 1;
        region.imageExtent                 = {mips[i].width, mips[i].height, 1};
        vkCmdCopyBufferToImage(cmd, staging, image, VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL, 1,
                               &region);
    }

    VkImageMemoryBarrier toRead = toDst;
    toRead.oldLayout      = VK_IMAGE_LAYOUT_TRANSFER_DST_OPTIMAL;
    toRead.newLayout      = VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL;
    toRead.srcAccessMask  = VK_ACCESS_TRANSFER_WRITE_BIT;
    toRead.dstAccessMask  = VK_ACCESS_SHADER_READ_BIT;
    vkCmdPipelineBarrier(cmd, VK_PIPELINE_STAGE_TRANSFER_BIT, VK_PIPELINE_STAGE_FRAGMENT_SHADER_BIT,
                         0, 0, nullptr, 0, nullptr, 1, &toRead);

    vkEndCommandBuffer(cmd);
    VkSubmitInfo si{VK_STRUCTURE_TYPE_SUBMIT_INFO};
    si.commandBufferCount = 1;
    si.pCommandBuffers    = &cmd;
    check_vk(vkQueueSubmit(g_Queue, 1, &si, VK_NULL_HANDLE));
    vkQueueWaitIdle(g_Queue);

    vkFreeCommandBuffers(g_Device, g_UploadPool, 1, &cmd);
    vkDestroyBuffer(g_Device, staging, g_Allocator);
    vkFreeMemory(g_Device, stagingMem, g_Allocator);

    // Per-mip view + ImGui descriptor set so the UI can draw any single level.
    tex.mipViews.resize(levels);
    tex.mipIds.resize(levels);
    for (uint32_t i = 0; i < levels; ++i)
    {
        VkImageViewCreateInfo vi{VK_STRUCTURE_TYPE_IMAGE_VIEW_CREATE_INFO};
        vi.image                       = image;
        vi.viewType                    = VK_IMAGE_VIEW_TYPE_2D;
        vi.format                      = VK_FORMAT_R8G8B8A8_UNORM;
        vi.subresourceRange.aspectMask = VK_IMAGE_ASPECT_COLOR_BIT;
        vi.subresourceRange.baseMipLevel = i;
        vi.subresourceRange.levelCount   = 1;
        vi.subresourceRange.layerCount   = 1;
        VkImageView view = VK_NULL_HANDLE;
        check_vk(vkCreateImageView(g_Device, &vi, g_Allocator, &view));
        tex.mipViews[i] = (uint64_t)view;
        VkDescriptorSet ds = ImGui_ImplVulkan_AddTexture(g_Sampler, view,
                                                         VK_IMAGE_LAYOUT_SHADER_READ_ONLY_OPTIMAL);
        tex.mipIds[i] = (void *)ds;
    }

    tex.image  = (uint64_t)image;
    tex.memory = (uint64_t)imgMem;
    tex.valid  = true;
    return tex;
}

void GpuContext::DestroyTexture(GpuTexture &tex)
{
    if (!tex.valid)
        return;
    vkDeviceWaitIdle(g_Device);
    for (void *id : tex.mipIds)
        if (id)
            ImGui_ImplVulkan_RemoveTexture((VkDescriptorSet)id);
    for (uint64_t v : tex.mipViews)
        if (v)
            vkDestroyImageView(g_Device, (VkImageView)v, g_Allocator);
    if (tex.image)
        vkDestroyImage(g_Device, (VkImage)tex.image, g_Allocator);
    if (tex.memory)
        vkFreeMemory(g_Device, (VkDeviceMemory)tex.memory, g_Allocator);
    tex = GpuTexture{};
}

void GpuContext::Shutdown()
{
    if (g_Device)
        vkDeviceWaitIdle(g_Device);
    if (g_Instance)
    {
        ImGui_ImplVulkan_Shutdown();
        ImGui_ImplGlfw_Shutdown();
        ImGui::DestroyContext();
        if (g_Sampler)
            vkDestroySampler(g_Device, g_Sampler, g_Allocator);
        if (g_UploadPool)
            vkDestroyCommandPool(g_Device, g_UploadPool, g_Allocator);
        ImGui_ImplVulkanH_DestroyWindow(g_Instance, g_Device, &g_MainWindowData, g_Allocator);
        if (g_DescriptorPool)
            vkDestroyDescriptorPool(g_Device, g_DescriptorPool, g_Allocator);
        if (g_Device)
            vkDestroyDevice(g_Device, g_Allocator);
        vkDestroyInstance(g_Instance, g_Allocator);
        g_Instance = VK_NULL_HANDLE;
    }
    if (win_)
        glfwDestroyWindow((GLFWwindow *)win_);
    glfwTerminate();
    win_ = nullptr;
}

} // namespace viewer
