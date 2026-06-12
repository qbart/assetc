# Asset Compiler

`assetc` compiles source assets under `assets/` (textures, meshes, shaders, materials, fonts) into runtime-friendly binary formats written to `runtime/`. Images become UASTC-encoded `.ktx2`, meshes become `.hmesh` with octahedral-packed normals/tangents and meshletized geometry; mesh files reference materials by stable 64-bit hashes so the engine can share resources across assets.

A glTF/GLB mesh additionally emits a **companion material table** (`.hmat`) plus one **`.ktx2` per referenced image**. The three layers are orthogonal: `.hmesh` is geometry, `.hmat` is the small per-source material descriptor table (PBR factors + texture refs), and `.ktx2` is the GPU-ready pixel payload. A submesh's `materialSlot` indexes straight into the `.hmat` table (`.hmat` row `i` ⇔ `SubMesh::materialSlot i`).

Texture refs in `.hmat`/`.hmesh` are stored as 64-bit hashes, not paths. A single global **manifest** (`runtime/assets.hman`) maps each hash back to the `.ktx2` file on disk, so the runtime can content-address textures (load the manifest once into a `hash → path` map, then resolve any `baseColorTex` etc.). See [docs/hman.md](docs/hman.md).

Every output format has a detailed binary spec under [`docs/`](#file-formats).

## Commands

| Command       | What it does                                                                          |
| ------------- | ------------------------------------------------------------------------------------- |
| `assetc`       | Compile everything under `assets/` into the output dir (default `runtime/`).          |
| `assetc init`  | Write a starter `assetc.yml` in the current directory (won't overwrite an existing one) and create the `assets/` source dir if missing. |
| `assetc info`  | Inspect the *compiled* output dir and print per-file stats + aggregate totals (no recompile). |
| `assetc check` | Verify cross-file integrity of the compiled output dir (exit non-zero on any problem). |
| `assetc pack info [file]` | Inspect a `.hpack` (default `<output>.hpack`): per-kind summary + entry listing. |
| `assetc ui [path]` | Open a Dear ImGui (Vulkan + GLFW) inspector on a `.hpack` or output dir: browse entries, **view every `.ktx2` mip level** (UASTC/Basis transcoded to RGBA on the fly, cube faces + array layers selectable), plus header info for the other formats. Defaults to the output dir, falling back to its sibling `<output>.hpack`. |

Common flags (apply to `assetc`; `-o` also applies to `info`/`check`):

- `-o, --output <dir>` — output directory (default `runtime`; overrides config/preset).
- `-j, --jobs <n>` — concurrent jobs.
- `--preset <name>` — use a named preset from `assetc.yml` (see Configuration).
- `--list-presets` — print the presets defined in `assetc.yml` and exit.
- `--verify` — re-read each written file and check structural validity.
- `--no-cache` — ignore the incremental build cache and rebuild everything.
- `--pack` — after building, bundle the whole output dir into a single `<output>.hpack`.

### Incremental builds

`assetc` keeps a content cache (`<output>/.assetc-cache`) keyed by each source's bytes + asset type + encoder version. On the next run an asset is skipped when its inputs are unchanged and its primary output still exists; the asset's manifest contributions are replayed from the cache so `assets.hman` stays complete without re-encoding. Bump `kEncoderVersion` (in `src/assetc/cache.hpp`) to invalidate the whole cache when an output format changes; delete the cache file or pass `--no-cache` to force a full rebuild.

`assetc info` reports geometry stats per `.hmesh` (verts / triangles / indices / meshlets / submeshes / materials / bounds), material-table breakdowns per `.hmat` (texture-slot usage, alpha modes, double-sided count), `.hman` manifest entry counts by kind/colorspace, and `.ktx2` dimensions / mips / format / supercompression — then a totals summary across the whole output tree.

`assetc check` validates internal consistency: each `.hmesh`/`.hmat`/`.hman` is structurally valid; every nonzero texture ref in a `.hmat` resolves via `.hman` to a file that exists; every `.hman` entry points at an existing file with a hash matching `HashAssetRef(path-without-".ktx2")`; and each `.hmesh` material count matches its companion `.hmat` row count with all submesh material slots in range. Use it as a CI gate after a build.

## Configuration (`assetc.yml`)

On startup `assetc` looks for the nearest `assetc.yml`, searching the working directory and its ancestors (so it can live at the project root). All keys are optional and fall back to built-in defaults. Run `assetc init` to drop a starter file.

```yaml
# Top-level: project-wide, not part of the layering.
input: assets            # source tree — same for every preset (default: assets)
output: runtime          # base output dir (default: runtime; overridden by -o)
pack: false              # bundle into <output>.hpack after building
preset: desktop          # preset used when --preset is omitted

# Base layer applied to every build.
default:
  mesh:
    merge: true          # bake glTF node transforms into one combined, world-space
                         # mesh; false keeps geometry source-local
  texture:
    compress: true       # UASTC-encode (default); false = raw R8G8B8A8 + Zstd
  rules:                 # cascading: later matches override earlier; `match` is a
                         # glob over the source-relative path (* spans '/', ? one char)
    - match: "ui/*"
      texture:
        compress: false  # keep UI / data textures pixel-exact

# Named presets overlay `default` when selected with --preset <name>.
presets:
  desktop:
    output: runtime/desktop
  mobile:
    output: runtime/mobile
    rules:
      - match: "decals/*"
        texture:
          compress: false
```

### Layering & precedence

Settings are resolved by **overlaying** layers from least to most specific; each layer only overrides the fields it sets:

1. built-in defaults
2. `default:` globals (`mesh` / `texture`)
3. selected preset's globals
4. `default.rules` (cascading, in file order)
5. preset's `rules` (cascading — preset rules win)

`input` and `pack` are read only at the top level. `output` is the base, and a **preset may redirect it** (`outputFor` = preset's `output` if set, else the base) so `--preset mobile` and `--preset desktop` build into separate dirs that never clobber. A CLI `-o/--output` overrides everything; `--pack` is OR-ed with the config.

### Presets

`--preset <name>` selects a preset (overriding the config's `preset:`); `--list-presets` prints the defined names; an unknown preset is a hard error listing the available ones. The active preset and resolved settings are folded into the incremental cache, so switching preset or editing a rule rebuilds exactly the affected assets.

### Settings (v1)

- **`mesh.merge`** — bake glTF node transforms into one combined mesh (true) vs keep source-local (false). No effect on skinned meshes (never baked) or OBJ (no node graph).
- **`texture.compress`** — `true` (default) UASTC-encodes; `false` writes a raw `R8G8B8A8` KTX2 (sRGB/linear per slot) with lossless Zstd — pixel-exact, for UI atlases, sprite sheets, and sRGB/data textures you want untouched. Applies to both standalone images and glTF-embedded textures.

Unknown keys are warned (typo protection). The schema is intentionally small — it's the place to grow further per-asset knobs (e.g. texture `max_size`, `channels`, mesh `lods`).

## Supported inputs

| Source pattern                         | Asset type | Output                                  |
| -------------------------------------- | ---------- | --------------------------------------- |
| `*.png` (default)                      | Color      | UASTC `.ktx2` (color mode)              |
| `*.n.png`                              | Normal     | UASTC `.ktx2` (normal mode)             |
| `*.ao.png`, `*.h.png`, `*.r.png`       | Grayscale  | UASTC `.ktx2` (grayscale mode)          |
| `*.lut.cube`                           | LUT        | `.lut.ktx2` (3D LUT)                    |
| `*.obj`, `*.gltf`, `*.glb`             | Mesh       | [`.hmesh`](docs/hmesh.md) (container); glTF also emits [`.hmat`](docs/hmat.md) + `tex_<i>.ktx2` |
| `*.shader/` (directory)                | Shader     | `<entryPoint>.spv` per Slang entry point (stage from `[shader(...)]`; names unique per folder) |
| `*.env/` (directory)                   | Cubemap    | UASTC `.env.ktx2` (6 faces: `px.png`, `nx.png`, `py.png`, `ny.png`, `pz.png`, `nz.png`) |
| `*.array/` (directory)                 | Array      | `.arr.ktx2` *(planned)*                 |
| `*.mat`                                | Material   | [`.hmat`](docs/hmat.md) *(planned standalone; today `.hmat` is emitted as a glTF companion)* |
| `*.ttf`, `*.otf`                       | Font       | [`.hfont`](docs/hfont.md) (glyph metrics + kerning) + a single-channel SDF `.ktx2` atlas |

For a glTF source `assets/models/chair.glb` the outputs are:

```
runtime/models/chair.hmesh          geometry + submesh table
runtime/models/chair.hmat           material table, row i == SubMesh::materialSlot i
runtime/models/chair/tex_<i>.ktx2   one UASTC .ktx2 per referenced glTF image i
runtime/models/chair.hanim          animation clips (only if the source is skinned + animated)
runtime/assets.hman                 global hash -> file manifest (all assets, written once)
```

Skinned glTF meshes also embed `SKIN` + `SKEL` chunks in the `.hmesh` (see [docs/hmesh.md](docs/hmesh.md#skinning-skin--skel)).

## File formats

All runtime formats are little-endian and versioned by a 4-byte magic in their header. Each has a full binary spec — field offsets, layout diagrams, and reader notes — under [`docs/`](docs/):

| Format | Magic | Spec | Summary |
| ------ | ----- | ---- | ------- |
| `.hmesh` | `HMSH` | [docs/hmesh.md](docs/hmesh.md) | Tagged-chunk geometry container: `DESC` + pure-array chunks (vertices, indices, meshlets, submeshes, material refs), optional skinning / LODs. |
| `.hanim` | `HANM` | [docs/hanim.md](docs/hanim.md) | Animation clips for a skinned `.hmesh`: per-joint TRS channels with keyframes. |
| `.hmat`  | `HMAT` | [docs/hmat.md](docs/hmat.md)   | Flat per-source PBR material table; fixed-stride `GpuMaterial[]`, textures referenced by hash. |
| `.hman`  | `HMAN` | [docs/hman.md](docs/hman.md)   | Single global hash → file manifest the runtime uses to content-address textures. |
| `.hfont` | `HFNT` | [docs/hfont.md](docs/hfont.md) | Font metadata (em-unit glyph metrics + kerning) beside a single-channel SDF `.ktx2` atlas for smooth, scalable text in 2D or 3D. |
| `.hpack` | `HPAK` | [docs/hpack.md](docs/hpack.md) | Optional bundle (`--pack`) of the whole output dir into one TOC + payload blob so the engine opens a single file. |

The resolution chain across formats: load `assets.hman` once into a `hash → path` map; load a mesh by its known path (`.hmesh`/`.hmat`/`.hanim` are siblings); a material's or font's texture hash resolves through the manifest to a `.ktx2` path (and, if bundled, through the `.hpack` TOC to the bytes).

## Using as a library (SDK)

The runtime-side loaders and format definitions live in a standalone **SDK** under
[`src/sdk/`](src/sdk/) so an engine can read `assetc` output without re-implementing
any parser. The SDK depends on **nothing but the C++23 standard library** — pulling
it in does *not* drag in slang, ktx, meshoptimizer, fmt, yaml-cpp, or Vulkan. The
`assetc` tool links this same SDK, so the encoder and your engine agree byte-for-byte
on every format.

It ships the format structs plus ready-made readers/validators: `ReadHMan`,
`ReadHAnim`, `ReadHFont`, `ReadPackToc`, the `Validate*` checks, the `.hmesh`/`.hmat`
struct layouts for zero-copy mmap, and `HashAssetRef` to resolve refs through the
manifest. Include everything via `<assetc/sdk.hpp>` or the individual headers.

### Integrate via CMake FetchContent

Point `FetchContent` at the `src/sdk` subdirectory with `SOURCE_SUBDIR` so it
configures only the standalone SDK and skips the rest of this repo (which fetches
slang, ktx, …):

```cmake
include(FetchContent)
FetchContent_Declare(assetc_sdk
    GIT_REPOSITORY https://github.com/qbart/assetc.git
    GIT_TAG        <tag-or-commit>
    SOURCE_SUBDIR  src/sdk)
FetchContent_MakeAvailable(assetc_sdk)

target_link_libraries(my_engine PRIVATE assetc::sdk)
```

```cpp
#include <assetc/sdk.hpp>

std::vector<assetc::ManifestEntry> manifest;
assetc::ReadHMan("runtime/assets.hman", manifest);     // hash -> path table
uint64_t id = assetc::HashAssetRef("models/chair/tex_0"); // resolve a ref
// ...then mmap the .hmesh / .hmat and cast the structs in <assetc/runtime_mesh.hpp>.
```

See [`src/sdk/README.md`](src/sdk/README.md) for the full header/entry-point table
and an `add_subdirectory` / install alternative.
