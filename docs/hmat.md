# .hmat file format (v1)

Little-endian flat material table. Magic `"HMAT"`. One `.hmat` is emitted per glTF source, alongside its [`.hmesh`](hmesh.md). It is a fixed-stride array of material rows in the **same dense slot order** as the mesh's `MTRL`/`SUBM` tables, so `SubMesh::materialSlot` indexes straight into it. The actual pixels live in companion `.ktx2` files, referenced by hash and resolved through the [manifest](hman.md).

## Layout

```
+--------------------------------+
| MatFileHeader (16 B)           |
+--------------------------------+
| GpuMaterial[count] (96 B each) |
+--------------------------------+
```

The file is exactly `16 + count * 96` bytes — no chunk table, directly mmappable as a `GpuMaterial[]`.

## MatFileHeader (16 B)

| Offset | Field     | Type | Notes                                            |
| -----: | --------- | ---- | ------------------------------------------------ |
|      0 | `magic`   | u32  | `"HMAT"`                                          |
|      4 | `version` | u32  | `1`                                              |
|      8 | `count`   | u32  | material rows (matches the companion `.hmesh` material count) |
|     12 | `flags`   | u32  | reserved                                         |

## GpuMaterial (96 B)

| Offset | Field                  | Type    | Notes                                                       |
| -----: | ---------------------- | ------- | ----------------------------------------------------------- |
|      0 | `baseColorFactor`      | f32 × 4 | linear RGBA multiplier                                       |
|     16 | `emissiveFactor`       | f32 × 3 | linear RGB                                                   |
|     28 | `metallicFactor`       | f32     |                                                             |
|     32 | `roughnessFactor`      | f32     |                                                             |
|     36 | `normalScale`          | f32     | glTF `normalTexture.scale`                                   |
|     40 | `occlusionStrength`    | f32     | glTF `occlusionTexture.strength`                             |
|     44 | `alphaCutoff`          | f32     | used when alpha mode == `MASK`                               |
|     48 | `flags`                | u32     | bit 0 = double-sided; bits 1-2 = alpha mode                 |
|     52 | `_pad`                 | u32     | aligns the texture refs to 8 bytes                          |
|     56 | `baseColorTex`         | u64     | FNV1a64 runtime ref of the `.ktx2`, `0` = none              |
|     64 | `metallicRoughnessTex` | u64     | ORM/MR packing: occlusion=R, roughness=G, metallic=B        |
|     72 | `normalTex`            | u64     |                                                             |
|     80 | `occlusionTex`         | u64     | equals `metallicRoughnessTex` when packed as ORM            |
|     88 | `emissiveTex`          | u64     |                                                             |

**Alpha mode** (flags bits 1-2): `0` = OPAQUE, `1` = MASK, `2` = BLEND.

Factors carry glTF defaults when a field is absent in the source (base color `1,1,1,1`; metallic/roughness `1`; emissive `0,0,0`; `normalScale`/`occlusionStrength` `1`; `alphaCutoff` `0.5`; OPAQUE, single-sided).

## Texture refs

Each `*Tex` field is an FNV1a64 hash of the texture's runtime ref, or `0` when the material has no texture in that slot. The ref is `<sourceRef>/tex_<imageIndex>` (lowercased, like material refs), and the matching file is written to `runtime/<rel-no-ext>/tex_<imageIndex>.ktx2`. Textures are **deduplicated by glTF image index** — one `.ktx2` per source image, even when shared across materials or slots.

## Texture color space & encoder mode

Each image is transcoded to UASTC `.ktx2` with the color space / swizzle dictated by the slot it is first used in:

| Slot                  | Mode          | OETF   | Swizzle | Notes                                  |
| --------------------- | ------------- | ------ | ------- | -------------------------------------- |
| baseColor, emissive   | `Color`       | sRGB   | none    | color data                             |
| metallicRoughness, occlusion | `LinearColor` | linear | none    | keeps all channels (ORM: O=R, R=G, M=B) |
| normal                | `Normal`      | linear | `rg01`  | X→R, Y→G; Z reconstructed in-shader     |

If the same image is pulled into conflicting color spaces by different materials, the first use wins and a warning is logged.

## Endianness

Little-endian only, same as `.hmesh` (`src/sdk/src/runtime_material.cpp` carries the matching `static_assert`).
