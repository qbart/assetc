# SDK

The `assetc` crate is the runtime side of assetc: a Rust library a game (or any
tool) uses to read what `assetc build` and `assetc pack` write. It knows
every [output format](../spec/), so a game never parses the binary layouts
itself. The idea follows the SDK of
[assetc-cpp](https://github.com/qbart/assetc-cpp); the API is assetc's own.

## Crate

- The only crate of this repository published to
  [crates.io](https://crates.io/crates/assetc): the package and the library
  are both `assetc` (`use assetc::Assets;`). It lives in `sdk/`, a member of
  the Cargo workspace.
- Lightweight: it depends on **nothing but `std`**. No Bevy, no glTF,
  meshoptimizer, image, shader or compression crates, and none of the
  compiler's code: it holds only what is needed to read the `.a*` formats at
  runtime. Pulling it in builds only the SDK.
- Engine-neutral: it returns plain Rust values (pixels as bytes, vertices as
  arrays). Turning them into engine resources (a Bevy `Image`, `Mesh`, …) is
  the game's part; the [demo](demo.md) shows how for Bevy.
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`sdk/LICENSE-MIT`, `sdk/LICENSE-APACHE`). Needs Rust 1.88 or newer
  (`rust-version`, checked with that toolchain).
- `sdk/README.md` is both its crates.io page and the front page of its API
  docs; its example is compiled as a doctest.

```toml
[dependencies]
assetc = "0.1"
# or, next to a checkout:
assetc = { path = "../assetc/sdk" }
```

The compiler (CLI and GUI) is the unpublished package `assetc-core` at the
repository root; its binary is still `assetc`. It reads the formats with its
own code and uses the SDK only in tests, to check that what it writes reads
back.

### Publishing

`make publish` runs `cargo publish -p assetc`: it checks that the crate
builds alone from its packaged files, then uploads it. It needs a crates.io
token (`cargo login`) and a version in `sdk/Cargo.toml` not published yet.
`make publish-check` (`cargo publish -p assetc --dry-run`) does everything
but the upload.

## Opening assets

`Assets` is the entry point: one value over either a pack or an output
directory, so a game can ship a pack and develop against loose files.

| Call | Does |
|------|------|
| `Assets::open(path)` | Opens a `.pak` file (by its `APAK` magic) or an output directory holding `assetc.aman.bin`. |
| `Assets::from_pack(bytes)` | A pack already in memory (`Vec<u8>`). |
| `assets.entries()` | Every asset: `Entry { id, kind, path }`, from the pack's contents or the directory's manifest. |
| `assets.of_kind(kind)` | The entries of one `Kind`, in path order. |
| `assets.entry(id)` / `assets.find(path)` | One entry by asset ID or by output path (`models/Fox.amesh.bin`). |
| `assets.read(id)` | The asset's bytes (`Cow<[u8]>`: borrowed from a pack, read from disk for a directory). |
| `assets.load::<T>(id)` | Reads and decodes: `T` is any format type below (`Texture`, `Mesh`, …). |

A texture's ID is the hash of its asset path without `.ktx2`, as in the
[manifest](../spec/aman.bin.md); materials, meshes, fonts and atlases refer
to textures and materials by these IDs, so `assets.load::<Texture>(id)`
resolves any reference found in another asset.

`Kind` is `Texture`, `Material`, `Mesh`, `Animation`, `Font`, `Schema`,
`Data`, `Graph`, `Tree`, `Atlas`, `Gradient`, `Palette`, `Shader`,
`CustomMaterial`, `Manifest` or `Other`, with the values of
[`.pak`](../spec/pak.md#contents) (`Shader` 13, `CustomMaterial` 14).

## Formats

Each format has a module with one type and a `decode(&[u8]) -> Result<T>`
function (also `T::decode`). Field names follow the format's spec; offsets
into string tables are resolved to `String`s, hashes are kept as `u64`.

| Module | Type | Reads | Spec |
|--------|------|-------|------|
| `pack` | `Pack`, `PackEntry` | `*.pak` | [`pak.md`](../spec/pak.md) |
| `manifest` | `Manifest`, `ManifestEntry` | `assetc.aman.bin` | [`aman.bin.md`](../spec/aman.bin.md) |
| `chunks` | `ChunkFile` | any chunked file: format name, version, chunks by name | [`chunks.md`](../spec/chunks.md) |
| `texture` | `Texture` | `*.ktx2`: format (`Rgba8Srgb`, `Rgba8Unorm`, `Rgba16Float`), size, depth, faces, layers, every mip level's bytes, and its `usage` (`assetc.usage`, `None` without it) | [`ktx2.md`](../spec/ktx2.md) |
| `material` | `Material` | `*.amat.bin` (version 1 or 2), with its struct data | [`amat.bin.md`](../spec/amat.bin.md) |
| `mesh` | `Mesh` | `*.amesh.bin`: vertices (normals and tangents unpacked to `f32`), indices, submeshes, material IDs, meshlets, levels of detail, skin, skeleton, locators, color and UV sets, properties, shapes, regions, paths | [`amesh.bin.md`](../spec/amesh.bin.md) |
| `anim` | `Animation` | `*.aanim.bin`: clips, channels with their keys, events; plus sampling a channel at a time | [`aanim.bin.md`](../spec/aanim.bin.md) |
| `font` | `Font` | `*.afont.bin`: metrics, glyphs by codepoint, kerning; plus laying out a line of text | [`afont.bin.md`](../spec/afont.bin.md) |
| `atlas` | `Atlas` | `*.aatlas.bin`: frames by name | [`aatlas.bin.md`](../spec/aatlas.bin.md) |
| `schema` | `Schema` | `*.aschema.bin` | [`aschema.bin.md`](../spec/aschema.bin.md) |
| `data` | `Data` | `*.adata.bin`: instances as trees of typed values, ref targets | [`adata.bin.md`](../spec/adata.bin.md) |
| `graph` | `Graph` | `*.agraph.bin` | [`agraph.bin.md`](../spec/agraph.bin.md) |
| `tree` | `Tree` | `*.atree.bin` | [`atree.bin.md`](../spec/atree.bin.md) |
| `gradient` | `GradientSet` | `*.agradient.bin`: gradients by name with their stops (sRGB bytes and linear floats), cells; plus sampling a gradient at `t` | [`agradient.bin.md`](../spec/agradient.bin.md) |
| `palette` | `Palette` | `*.apalette.bin`: colors by name (sRGB bytes and linear floats), cells | [`apalette.bin.md`](../spec/apalette.bin.md) |
| `shader` | `Shader` | `*.ashader.bin`: params type, GPU block fields by path, textures, state, bindings, SPIR-V words; see [Custom materials](#custom-materials) | [`ashader.bin.md`](../spec/ashader.bin.md) |
| `custom_material` | `CustomMaterial` | `*.acmat.bin`: shader ID, state, GPU block bytes, texture IDs, params and attached structs | [`acmat.bin.md`](../spec/acmat.bin.md) |
| `structs` | `Structs` | the [struct chunks](../spec/astruct.md) of any format above | [`astruct.md`](../spec/astruct.md) |
| `hash` | `fnv1a64(bytes)` | the hash behind every asset ID and name hash | [`build.md`](build.md#asset-ids) |

## Attached structs

Every format that can carry [attached structs](structs.md) (`Graph`,
`Tree`, `Mesh`, `Material`, `Atlas`, `GradientSet`, `Palette`,
`CustomMaterial`) has a `structs: Structs` field, empty without them:

| Call | Gives |
|------|-------|
| `structs.all()` | every `Attached { scope, item, type_name, type_hash, schema, value }`, in file order |
| `structs.of(scope, item)` | the structs of one owner: `Scope::Asset` (item `0`), `Scope::Item` (nodes, gradients, colors), `Scope::Edge` or `Scope::Params` (a custom material's params, item `0`); `Scope::Other(n)` keeps scopes the SDK does not know |
| `structs.get(scope, item, "Type")` | one struct of one owner, by name |
| `attached.value` | the struct's value, a [`data::Value`](#formats) `Struct` with every field resolved; `named(&schema)` names its fields with the struct's schema, loaded by `attached.schema` |

## Custom materials

A [custom material](custom-materials.md) is two assets: the shader's
`Shader` (its interface, shared by every material of it) and the material's
`CustomMaterial` (its values). A game loads the shader by the material's
`shader` ID, builds the pipeline once per shader and fills one bind group per
material.

`Shader` (`*.ashader.bin`):

| Field / call | Gives |
|--------------|-------|
| `params_type`, `params_type_hash`, `params_schema` | the params struct's name, its FNV-1a-64 and its schema's asset ID (`""`, `0`, `0` without params); `has_params()` |
| `block_size` | the GPU block's size in bytes (multiple of 16; `0` without a block) |
| `state` | `State { blend, cull, depth_write, depth_test, alpha_cutoff }`: `Blend::{Opaque, Mask, Blend, Add, Multiply}`, `Cull::{Back, Front, None}`, `DepthTest::{Less, LessEqual, Equal, Always}`; `State::bits()` gives the [state bits](../spec/ashader.bin.md#state-bits) back |
| `fields` / `field(path)` | the block's values in offset order: `Field { path_hash, path, offset, ty, source }`, nested fields by their joined path (`foam.amount`); `ty` is `FieldType::{U32, I32, F32, Vec3, Vec4}` (`size()` in bytes), `source` the schema type it came from, `FieldSource::{Bool, Integer, Float, Enum, Rgb, Rgba}` |
| `textures` / `texture(name)` / `texture_index(name)` | `ShaderTexture { name_hash, name, usage, dimension, default, srgb, required, bare, mipmaps, pack }`, in the shader's (and the material's `textures`) order; `usage` is `Usage::{Color, Data, …}` (`key()` is the KTX2 [usage key](../spec/ktx2.md)), `dimension` `Dimension::{D2, D3, Cube}`, `default` `Fallback::{White, Black, Gray, Normal, Transparent}`, `pack` `Some(Pack { inputs, fill })` when packed: four `Option<PackInput { name, channel }>` for R, G, B and A (`None` where no input fills it, `channel` `0`–`3` for r–a) and the fill bytes |
| `bindings` / `binding(name)` / `binding_for(kind, texture)` | the resources the stages declare: `Binding { kind, texture, set, binding, stages, size, name }`, `kind` `BindingKind::{Params, Frame, Object, Texture, Sampler}`, `texture` the index into `textures` (kinds `Texture`, `Sampler`), `stages` bits (`STAGE_VERTEX` 1, `STAGE_FRAGMENT` 2; `in_vertex()`, `in_fragment()`), `size` the uniform block's declared size; `binding_for(BindingKind::Params, 0)` finds the params block whatever its name |
| `vertex`, `fragment` | `Option<Vec<u32>>`: each stage's SPIR-V words, ready for `wgpu::ShaderSource::SpirV` or `vkCreateShaderModule`; `None` without the stage |
| `vertex_entry`, `fragment_entry` | the stages' entry points (`""` without the stage) |
| `doc` | the shader's `doc` |

`CustomMaterial` (`*.acmat.bin`):

| Field / call | Gives |
|--------------|-------|
| `shader` | the asset ID of its `Shader` |
| `state` | the shader's `State` with the material's overrides applied |
| `params` | `Vec<u8>`: the GPU block, `block_size` bytes of std140, to upload as the `Params` uniform buffer as is (empty without a block) |
| `textures` / `texture(&shader, name)` | the texture asset IDs, in the shader's texture order; `0` is empty: bind the texture's `default` |
| `structs` | the params and the attached structs ([above](#attached-structs)) |
| `params_struct()` / `param(name)` | the params as an `Attached` (`Scope::Params`, item `0`; `None` without params), and one of its fields, every field resolved, for the CPU side |

The block's layout and the textures' kinds are in the `Shader` only; the
material does not repeat them.

## Errors

Every reader returns `Result<T, assetc::Error>`:

- `Error::Io` for a file that cannot be read;
- `Error::Format { file, message }` for bytes that are not what the spec
  says: a wrong magic or format name, an unsupported version, a missing
  required chunk, a count or range past the end of the data, text that is
  not UTF-8;
- `Error::NotFound` for an ID or path the assets do not hold.

Readers check every count, offset and length against the data before
using it and never panic on bad input. Like the specs ask, they ignore
chunks they do not know.

## Conformance

The SDK is tested against the compiler, not only against itself: assetc's
test suite builds and packs the [samples](../../samples), opens the pack and
the output directory with `Assets::open`, and decodes every entry with the
type of its kind. A format change that the SDK does not follow fails that
test.

The SDK's own tests decode small files written by hand from the specs.
