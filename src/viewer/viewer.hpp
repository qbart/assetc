#pragma once

#include <string>

namespace assetc
{

// Open the Dear ImGui (Vulkan) asset inspector on `path`, which may be a
// `.hpack` bundle or a compiled output directory. Browses entries, decodes
// `.ktx2` textures (transcoding UASTC/Basis to RGBA8) and shows every mip
// level, and prints header info for the other runtime formats.
//
// Returns 0 on clean exit, non-zero if the window/Vulkan could not be created
// or the path is unreadable (a logged reason is printed in that case).
int RunViewer(const std::string &path);

} // namespace assetc
