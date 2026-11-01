# `.amat` materials

One compiled PBR material (glTF 2.0 metallic-roughness), written to
`<output>/<dir>/<name>.amat` (from `*.mat.toml` and texture groups), or
`<output>/<model>/<material>.amat` (from a glTF model); see
[`docs/features/materials.md`](../features/materials.md) and
[`docs/features/models.md`](../features/models.md#build).

The file is a single fixed-size record, so it can be read straight into a
struct or uploaded to a GPU buffer. Textures are referenced by asset ID and
resolved through [`assets.manifest`](manifest.md).

## Encoding

Little-endian, 112 bytes, no padding other than listed. Floats are IEEE 754
`f32`.

| Offset | Size | Field                | Notes                                         |
|-------:|-----:|----------------------|-----------------------------------------------|
|      0 |    4 | `magic`              | ASCII `ACMT` (`41 43 4D 54`)                  |
|      4 |    4 | `version`            | u32 `1`                                       |
|      8 |   16 | `base_color`         | f32 × 4, linear RGBA factor                   |
|     24 |   12 | `emissive`           | f32 × 3, linear RGB factor                    |
|     36 |    4 | `emissive_strength`  | f32                                           |
|     40 |    4 | `metallic`           | f32 factor                                    |
|     44 |    4 | `roughness`          | f32 factor                                    |
|     48 |    4 | `occlusion_strength` | f32                                           |
|     52 |    4 | `normal_scale`       | f32                                           |
|     56 |    4 | `height_scale`       | f32                                           |
|     60 |    4 | `height_midlevel`    | f32                                           |
|     64 |    4 | `alpha_cutoff`       | f32                                           |
|     68 |    4 | `flags`              | u32, see below                                |
|     72 |    8 | `base_color_texture` | u64 asset ID, sRGB texture                    |
|     80 |    8 | `normal_texture`     | u64 asset ID, linear, +Y up                   |
|     88 |    8 | `orm_texture`        | u64 asset ID, linear, R occlusion, G roughness, B metallic |
|     96 |    8 | `emissive_texture`   | u64 asset ID, sRGB                            |
|    104 |    8 | `height_texture`     | u64 asset ID, linear, white = high            |

A texture ID of `0` means "no texture". ID `0` is reserved: the build fails
if any asset path hashes to it.

### `flags`

| Bits | Meaning                                                     |
|------|-------------------------------------------------------------|
| 0–1  | alpha mode: `0` opaque, `1` mask, `2` blend (`3` reserved)  |
| 2    | double-sided                                                |
| 3    | height mode: `0` parallax, `1` displacement                 |
| 4–31 | reserved, `0`                                               |

## Validation

A reader should reject the file when its size is not 112, the magic or
version does not match, or the alpha mode is `3`.

## Shading notes

As in glTF: `base_color = base_color_factor × texture`,
`roughness = roughness × orm.g`, `metallic = metallic × orm.b`,
`occlusion = 1 + occlusion_strength × (orm.r − 1)`,
`emissive = emissive × emissive_strength × texture`.
Displacement offsets along the normal by
`height_scale × (height − height_midlevel)`.
