# SDK

`assetc-sdk` is the runtime side of assetc: a Rust library a game (or any
tool) uses to read what `assetc build` and `assetc pack` write. It knows
every [output format](../spec/), so a game never parses the binary layouts
itself. The idea follows the SDK of
[assetc-cpp](https://github.com/qbart/assetc-cpp); the API is assetc's own.

## Crate

- Lives in `sdk/` of this repository, a member of the Cargo workspace; the
  package is `assetc-sdk`, the library `assetc_sdk`.
- Depends on **nothing but `std`**: no Bevy, no assetc, no image or
  compression crates. Pulling it in builds only the SDK.
- Engine-neutral: it returns plain Rust values (pixels as bytes, vertices as
  arrays). Turning them into engine resources (a Bevy `Image`, `Mesh`, …) is
  the game's part; the [demo](demo.md) shows how for Bevy.

```toml
[dependencies]
assetc-sdk = { git = "<this repository's URL>", package = "assetc-sdk" }
# or, next to a checkout:
assetc-sdk = { path = "../assetc/sdk" }
```

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
`Data`, `Graph`, `Tree`, `Atlas`, `Manifest` or `Other`, with the values of
[`.pak`](../spec/pak.md#contents).

## Formats

Each format has a module with one type and a `decode(&[u8]) -> Result<T>`
function (also `T::decode`). Field names follow the format's spec; offsets
into string tables are resolved to `String`s, hashes are kept as `u64`.

| Module | Type | Reads | Spec |
|--------|------|-------|------|
| `pack` | `Pack`, `PackEntry` | `*.pak` | [`pak.md`](../spec/pak.md) |
| `manifest` | `Manifest`, `ManifestEntry` | `assetc.aman.bin` | [`aman.bin.md`](../spec/aman.bin.md) |
| `chunks` | `ChunkFile` | any chunked file: format name, version, chunks by name | [`chunks.md`](../spec/chunks.md) |
| `texture` | `Texture` | `*.ktx2`: format (`Rgba8Srgb`, `Rgba8Unorm`, `Rgba16Float`), size, depth, faces, every mip level's bytes | [`ktx2.md`](../spec/ktx2.md) |
| `material` | `Material` | `*.amat.bin` | [`amat.bin.md`](../spec/amat.bin.md) |
| `mesh` | `Mesh` | `*.amesh.bin`: vertices (normals and tangents unpacked to `f32`), indices, submeshes, material IDs, meshlets, levels of detail, skin, skeleton, locators, color and UV sets, properties, shapes, regions, paths | [`amesh.bin.md`](../spec/amesh.bin.md) |
| `anim` | `Animation` | `*.aanim.bin`: clips, channels with their keys, events; plus sampling a channel at a time | [`aanim.bin.md`](../spec/aanim.bin.md) |
| `font` | `Font` | `*.afont.bin`: metrics, glyphs by codepoint, kerning; plus laying out a line of text | [`afont.bin.md`](../spec/afont.bin.md) |
| `atlas` | `Atlas` | `*.aatlas.bin`: frames by name | [`aatlas.bin.md`](../spec/aatlas.bin.md) |
| `schema` | `Schema` | `*.aschema.bin` | [`aschema.bin.md`](../spec/aschema.bin.md) |
| `data` | `Data` | `*.adata.bin`: instances as trees of typed values, ref targets | [`adata.bin.md`](../spec/adata.bin.md) |
| `graph` | `Graph` | `*.agraph.bin` | [`agraph.bin.md`](../spec/agraph.bin.md) |
| `tree` | `Tree` | `*.atree.bin` | [`atree.bin.md`](../spec/atree.bin.md) |
| `hash` | `fnv1a64(bytes)` | the hash behind every asset ID and name hash | [`build.md`](build.md#asset-ids) |

## Errors

Every reader returns `Result<T, assetc_sdk::Error>`:

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
