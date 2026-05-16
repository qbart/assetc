#pragma once

#include "runtime_mesh.hpp"

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

namespace obj
{
struct OBJ;
}

namespace assetc
{

struct CompiledMesh
{
    Mesh                  mesh;         // GpuVertex array + indices + meshlets
    MeshBounds            bounds;       // mesh-wide AABB + sphere
    std::vector<uint64_t> materialRefs; // FNV1a64 hashes of material runtime refs
};

// Build a runtime-ready mesh from a parsed OBJ.
//
// `meshRef` is the per-mesh runtime key in canonical form (forward slashes,
// lowercase, no leading "runtime/", no extension). Example: a source asset at
// `assets/models/chair.obj` becomes `meshRef = "models/chair"`. This key is
// used as the namespace for material refs: each unique material referenced by
// the mesh hashes to `HashAssetRef(meshRef + "/" + material_name_lowercased)`.
//
// On failure, the returned CompiledMesh has empty vectors.
CompiledMesh BuildFromObj(const obj::OBJ &src, std::string_view meshRef);

// FNV1a64 hash of a runtime-relative asset reference. Caller-side normalization
// (lowercase, /-separated, no extension, no leading "runtime/" prefix) is the
// authoritative format; this function additionally lowercases ASCII A-Z for
// belt-and-braces consistency.
uint64_t HashAssetRef(std::string_view runtimeRefNoExt) noexcept;

} // namespace assetc
