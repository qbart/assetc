# .hman file format (v1)

Little-endian. Magic `"HMAN"`. A **single global** `runtime/assets.hman` is written once per build (not per source), after every asset has compiled successfully. It maps each texture's FNV1a64 runtime ref — the exact hash already stored in [`.hmat`](hmat.md)/[`.hmesh`](hmesh.md)/[`.hfont`](hfont.md) — to its `.ktx2` path, so the runtime can resolve `baseColorTex` and friends to bytes on disk.

The engine loads it once into a `hash → path` map and does `root + "/" + path` to open each texture. Entries are sorted by hash ascending, so a reader may `lower_bound` instead of building a map.

## Layout

```
+-------------------------------------+
| ManHeader (16 B)                    |
+-------------------------------------+
| ManEntry[count] (variable length)   |
+-------------------------------------+
```

## ManHeader (16 B)

| Offset | Field      | Type | Notes                       |
| -----: | ---------- | ---- | --------------------------- |
|      0 | `magic`    | u32  | `"HMAN"`                    |
|      4 | `version`  | u32  | `1`                         |
|      8 | `count`    | u32  | number of entries           |
|     12 | `reserved` | u32  | `0`                         |

## ManEntry (variable length)

| Field        | Type           | Notes                                                          |
| ------------ | -------------- | -------------------------------------------------------------- |
| `hash`       | u64            | FNV1a64 ref — byte-identical to the `.hmat`/`.hmesh` ref hash  |
| `kind`       | u8             | `0` texture, `1` mesh, `2` material, `4` shader, `5` embed, `6` animation, `7` font (`3` lut reserved) |
| `colorspace` | u8             | `0` = linear, `1` = sRGB                                       |
| `pathLen`    | u16            | byte length of `path`                                          |
| `path`       | `u8[pathLen]`  | UTF-8, forward-slash, **relative to the runtime root**         |

Entries are streamed (not a fixed stride), so the file is parsed sequentially rather than mmap-cast.

## Canonical hashed string

There are two texture-entry shapes:

- **Content-addressed textures** (glTF-embedded images) live in the shared flat store. The `hash` is the content hash (image bytes + encoder mode; see [.hmat texture refs](hmat.md#texture-refs)) and the `path` is `tex/<hash>.ktx2` with the hash as 16 lowercase hex digits, so `hash == parse_hex(stem of path)`. Identical textures across sources share one entry.
- **Name-addressed textures** (font SDF atlases) keep `hash == HashAssetRef(path-without-".ktx2")`, where the ref is the source path relative to `assets/`, extension stripped, lowercased, `/`-separated.

Shaders (`kind = 4`) follow the name-addressed shape with `path = "<sourceRef>/<entryPoint>.spv"` and `hash == HashAssetRef(path-without-".spv")`.

**Embeds** (`kind = 5`) are raw files copied verbatim into the runtime tree by the `embed:` config option (see the main [README](../README.md#configuration-assetcyml)). Unlike the texture/shader refs, the **extension is part of the hashed string**: `hash == HashEmbedRef(path)`, where `path` is the full runtime-relative path *with* its extension (e.g. `scene/level.json`), lowercased, `/`-separated. So `scene/level.json` and `scene/level.xml` get distinct ids. The engine reads an embed by `HashEmbedRef("scene/level.json") → path → bytes` (loose or via the `.hpack` TOC). `colorspace` is `linear` and unused.

**By-path assets** — meshes (`kind = 1`, `.hmesh`), material tables (`kind = 2`, `.hmat`), animation clips (`kind = 6`, `.hanim`), and font metadata (`kind = 7`, `.hfont`) — use the **same `HashEmbedRef(path)` rule as embeds**: the hash is over the full runtime-relative path with extension. Because a mesh source emits `<stem>.hmesh`/`.hmat`/`.hanim` sharing a stem, keeping the extension is what gives each a distinct id. The engine resolves any of them uniformly: `HashEmbedRef("models/chair.hmesh") → path → bytes`.

## Scope and guarantees

- **What appears.** Every compiled asset is now resolvable by id: content-addressed `.ktx2` images referenced by an emitted `.hmat` (in `tex/`), **font SDF atlases** + **font metadata** ([`.hfont`](hfont.md)), **shader entry points** (one per Slang `[shader(...)]`), **meshes** ([`.hmesh`](hmesh.md)), **material tables** ([`.hmat`](hmat.md)), **animation clips** ([`.hanim`](hanim.md)), and **embeds** (every file matched by the `embed:` config globs). Still excluded: standalone `*.png`-compiled textures (Color/Normal/Grayscale) and LUTs — they're rarely referenced by id; `kind` reserves room to add them the same way.
- **Colorspace** mirrors what `assetc` baked into the `.ktx2` (sRGB for the `Color` slot, linear otherwise) — authoritative, so the runtime need not re-derive it from the material slot.
- **Deduplicated.** A ref reachable from multiple slots/assets appears once; identical `(hash, path)` pairs collapse.
- **Collision-checked.** If two distinct paths hash equal the build fails loudly rather than emit an ambiguous entry.
- **Deterministic.** Same inputs → byte-identical `assets.hman` (sorted output, no timestamps).

## Endianness

Little-endian only (`src/sdk/src/runtime_manifest.cpp` carries the matching `static_assert`).
