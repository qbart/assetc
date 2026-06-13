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
| `kind`       | u8             | `0` = texture (`1` mesh, `2` material, `3` lut reserved)       |
| `colorspace` | u8             | `0` = linear, `1` = sRGB                                       |
| `pathLen`    | u16            | byte length of `path`                                          |
| `path`       | `u8[pathLen]`  | UTF-8, forward-slash, **relative to the runtime root**         |

Entries are streamed (not a fixed stride), so the file is parsed sequentially rather than mmap-cast.

## Canonical hashed string

There are two texture-entry shapes:

- **Content-addressed textures** (glTF-embedded images) live in the shared flat store. The `hash` is the content hash (image bytes + encoder mode; see [.hmat texture refs](hmat.md#texture-refs)) and the `path` is `tex/<hash>.ktx2` with the hash as 16 lowercase hex digits, so `hash == parse_hex(stem of path)`. Identical textures across sources share one entry.
- **Name-addressed textures** (font SDF atlases) keep `hash == HashAssetRef(path-without-".ktx2")`, where the ref is the source path relative to `assets/`, extension stripped, lowercased, `/`-separated.

## Scope and guarantees

- **Textures only.** Only `.ktx2` images referenced by an emitted `.hmat` (content-addressed in `tex/`), plus **font SDF atlases** referenced by an emitted [`.hfont`](hfont.md), appear. The standalone `*.png`-compiled textures (Color/Normal/Grayscale) and LUTs are not listed; `kind` reserves room to add them later.
- **Colorspace** mirrors what `assetc` baked into the `.ktx2` (sRGB for the `Color` slot, linear otherwise) — authoritative, so the runtime need not re-derive it from the material slot.
- **Deduplicated.** A ref reachable from multiple slots/assets appears once; identical `(hash, path)` pairs collapse.
- **Collision-checked.** If two distinct paths hash equal the build fails loudly rather than emit an ambiguous entry.
- **Deterministic.** Same inputs → byte-identical `assets.hman` (sorted output, no timestamps).

## Endianness

Little-endian only (`src/sdk/src/runtime_manifest.cpp` carries the matching `static_assert`).
