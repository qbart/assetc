# assetc SDK

The **runtime-side** API for loading assets compiled by [`assetc`](../../README.md).
Drop it into an engine and you get the on-disk format definitions plus ready-made
loaders/validators — no need to reverse-engineer the binary layouts or write your
own parsers.

It depends on **nothing but the C++23 standard library**. Pulling in the SDK does
*not* drag in slang, ktx, meshoptimizer, fmt, yaml-cpp, Vulkan, or any of the
compiler-side machinery the `assetc` tool uses. One include path, one static lib.

## What's in it

| Header (`#include <assetc/…>`) | Covers | Key entry points |
|---|---|---|
| `runtime_mesh.hpp`     | `.hmesh` geometry (chunked, mmap-friendly) | structs (`GpuVertex`, `MeshDesc`, `SubMesh`, `GpuJoint`…), `ValidateHMesh`, `OctEncode` |
| `runtime_material.hpp` | `.hmat` PBR material table | `GpuMaterial`, `ReadHMat`-via-struct, `ValidateHMat` |
| `runtime_manifest.hpp` | `assets.hman` hash → file table | `ManifestEntry`, `ReadHMan`, `ValidateHMan` |
| `runtime_anim.hpp`     | `.hanim` skeletal animation | `AnimClip`/`AnimChannel`, `ReadHAnim`, `ValidateHAnim` |
| `runtime_font.hpp`     | `.hfont` SDF font metrics | `FontFileHeader`/`GpuGlyph`, `ReadHFont`, `ValidateHFont` |
| `pack.hpp`             | `.hpack` bundle TOC | `PackEntry`, `ReadPackToc`, `ValidatePack`, `PackKindOf` |
| `hash.hpp`             | ref → id hashing | `HashAssetRef` |
| `sdk.hpp`              | umbrella that includes all of the above | — |

The same headers and the same `WriteHMesh`/`WriteHMan`/… serializers back the
`assetc` compiler itself, so the encoder and your engine agree byte-for-byte on
every format. (The compiler links this SDK; the SDK never links back.)

## Typical engine flow

```cpp
#include <assetc/sdk.hpp>

// 1. Load the manifest (hash -> runtime-relative path).
std::vector<assetc::ManifestEntry> manifest;
assetc::ReadHMan("runtime/assets.hman", manifest);

// 2. Resolve a material's texture ref to a file.
uint64_t ref = mat.baseColorTex;                 // from a GpuMaterial in a .hmat
// ...look ref up in the manifest map you built from `manifest`...

// 3. Or compute a ref id yourself from a canonical ref string.
uint64_t id = assetc::HashAssetRef("models/chair/tex_0");

// 4. mmap a .hmesh and walk its chunks using the structs in runtime_mesh.hpp.
```

## Integrate via CMake FetchContent

Point FetchContent at this subdirectory with `SOURCE_SUBDIR` so it configures the
standalone SDK project and skips the rest of the assetc repo (which would fetch
slang, ktx, …):

```cmake
include(FetchContent)
FetchContent_Declare(assetc_sdk
    GIT_REPOSITORY https://example.com/assetc.git
    GIT_TAG        <tag-or-commit>
    SOURCE_SUBDIR  src/sdk)
FetchContent_MakeAvailable(assetc_sdk)

target_link_libraries(my_engine PRIVATE assetc::sdk)
```

That exposes the `assetc::sdk` target (a C++23 static library) and the
`<assetc/…>` headers. Nothing else is fetched or built.

### Or add_subdirectory / install

```cmake
add_subdirectory(path/to/assetc/src/sdk)
target_link_libraries(my_engine PRIVATE assetc::sdk)
```

Building `src/sdk` as a top-level project also emits `install()` rules
(`assetc::sdk` target + headers + a CMake package export).
