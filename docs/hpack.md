# .hpack file format (v2)

Little-endian. Magic `"HPAK"`. `assetc --pack` bundles every runtime file into a single `<output>.hpack` (e.g. `runtime.hpack`) written next to the output dir, so the engine opens one file instead of thousands. It is a table of contents followed by 16-byte-aligned payloads.

```
PackHeader (24 B):
    magic    u32  'HPAK'
    version  u32  2
    count    u32  number of entries
    flags    u32  reserved (0)
    tocBytes u64  size of the TOC region after the header
TOC: count entries, sorted by path ascending:
    offset   u64  payload byte offset from file start (16-aligned)
    size     u64  payload bytes
    kind     u8   PackKind (asset type, see below)
    pathLen  u16
    path     bytes  UTF-8, forward-slash, relative to the output root
Payloads: each file's bytes at its offset, padded to 16-byte alignment.
```

`kind` is derived from the path extension at pack time so the engine can filter/dispatch by type without string-sniffing:

| Value | `PackKind` | Extension |
| ----: | ---------- | --------- |
| 0 | `Other`     | (anything else) |
| 1 | `Mesh`      | `.hmesh` |
| 2 | `Material`  | `.hmat` |
| 3 | `Manifest`  | `.hman` |
| 4 | `Animation` | `.hanim` |
| 5 | `Texture`   | `.ktx2` |
| 6 | `Shader`    | `.spv` |
| 7 | `Font`      | `.hfont` |

The engine loads the TOC once into a `path → (offset, size, kind)` map (or binary-searches the sorted TOC) and reads/mmaps each entry in place — `path` matches the runtime-relative paths used elsewhere (e.g. `.hman` entries, `models/court/tex_0.ktx2`). Resolution chain: load `assets.hman` from the pack → `hash → path` map; load a mesh by its known path (`<stem>.hmesh`/`.hmat`/`.hanim` are siblings); a material's texture hash resolves via the manifest to a path, then via the TOC to the `.ktx2` bytes (a shader entry point's `<sourceRef>/<entryPoint>` hash resolves to its `.spv` the same way). Excluded from the pack: the build cache (`.assetc-cache`) and any existing `.hpack`. Deterministic: the same output tree produces byte-identical pack bytes (entries sorted, alignment padding zeroed). `--verify` runs `ValidatePack` (every entry's range lies within the file).

Inspect a pack without unpacking it with `assetc pack info [file]` (defaults to `<output>.hpack`): it prints a layout breakdown (header / TOC / payload / alignment padding), a per-kind summary (meshes / materials / manifests / animations / textures / shaders / fonts), and a path-sorted entry listing where each row is tagged with its `PackKind`, size, and byte offset.
