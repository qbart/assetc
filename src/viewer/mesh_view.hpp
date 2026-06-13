#pragma once

// CPU-side .hmesh preview for the `assetc ui` inspector.
//
// The viewer has no GPU mesh pipeline (it is a Dear ImGui 2D inspector); meshes
// are previewed with a small software rasterizer that projects triangles on the
// CPU and draws them through ImGui's draw list. That keeps the viewer free of
// extra shaders/pipelines and makes LOD differences obvious: flipping the LOD
// selector swaps which index set is drawn, so you literally watch the triangle
// budget collapse.

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace viewer
{

// Parsed geometry: a shared position buffer plus one index list per LOD level.
// `lodIndices[0]` is the full-resolution mesh (LOD0 from IDXS); higher levels are
// the reduced LODs (LODI/LODT), already expanded with previous-level fallback so
// every level is directly drawable.
struct MeshCpu
{
    bool        valid = false;
    std::string error;

    std::vector<float>                 positions; // 3 floats per vertex (object space)
    uint32_t                           vertexCount = 0;
    std::vector<std::vector<uint32_t>> lodIndices; // [level] -> global vertex indices

    uint32_t submeshCount = 0;
    uint32_t materialCount = 0;

    float aabbMin[3] = {0, 0, 0};
    float aabbMax[3] = {0, 0, 0};
    float center[3]  = {0, 0, 0};
    float radius     = 1.0f;
};

// Decode a .hmesh blob into CPU geometry + per-LOD index lists. On failure the
// result has valid=false and `error` set.
MeshCpu ParseHMesh(const uint8_t *data, size_t size);

// Orbit camera + render settings, persisted across frames by the caller.
struct MeshCamera
{
    bool  initialized = false;
    float yaw         = 0.7f; // radians, around world up
    float pitch       = 0.5f; // radians, clamped away from the poles
    float distScale   = 1.0f; // multiple of the auto-fit distance
    int   lod         = 0;    // selected LOD level (index into MeshCpu::lodIndices)
    bool  solid       = true; // filled, flat-shaded triangles
    bool  wire        = true; // triangle edges
};

// Draw the LOD selector, render-mode toggles, and the interactive orbit preview
// inside the current ImGui window. Consumes mouse drag/wheel over the canvas.
void DrawMeshPreview(const MeshCpu &mesh, MeshCamera &cam);

} // namespace viewer
