# `<name>.mat.toml`

Optional material definition next to a material's textures; see
[`docs/features/materials.md`](../features/materials.md) for how materials are
found and built.

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. File name `<name>.mat.toml`,
where `<name>` is the material name (case-sensitive).

## Schema

Every key is optional; an empty file is a valid material that uses the
defaults. Unknown keys are rejected.

| Key                  | Type               | Range / values                  | Default (see materials.md) |
|----------------------|--------------------|---------------------------------|----------------------------|
| `base_color`         | array of 4 floats  | each `0..=1`, linear RGBA        | `[1, 1, 1, 1]`             |
| `metallic`           | float              | `0..=1`                         | `1.0` with a map, else `0.0` |
| `roughness`          | float              | `0..=1`                         | `1.0`                      |
| `occlusion_strength` | float              | `0..=1`                         | `1.0`                      |
| `normal_scale`       | float              | any finite                      | `1.0`                      |
| `normal_y`           | string             | `"gl"` (+Y up), `"dx"` (+Y down) | `"gl"`                    |
| `emissive`           | array of 3 floats  | each `0..=1`, linear RGB         | `[0, 0, 0]`, or `[1, 1, 1]` with a map |
| `emissive_strength`  | float              | `>= 0`                          | `1.0`                      |
| `height_mode`        | string             | `"parallax"`, `"displacement"`  | `"parallax"`               |
| `height_scale`       | float              | `>= 0`                          | `0.05`                     |
| `height_midlevel`    | float              | `0..=1`                         | `0.5`                      |
| `alpha_mode`         | string             | `"opaque"`, `"mask"`, `"blend"` | `"opaque"`                 |
| `alpha_cutoff`       | float              | `0..=1`, used by `"mask"`       | `0.5`                      |
| `double_sided`       | bool               |                                 | `false`                    |
| `[textures]`         | table              | see below                       | empty                      |

All floats must be finite. A value out of range fails the material.

### `[textures]`

Keys are slot names: `base_color`, `normal`, `roughness`, `metallic`,
`occlusion`, `emissive`, `height`. Values are paths to texture sources:

- relative to the directory holding the `.mat.toml`, `/`-separated;
- `.` and `..` are allowed but the result must stay inside `input`;
- the file must exist and be a texture source (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`).

A key here replaces the convention file for that slot.

## Example

```toml
# samples/brick/brick.mat.toml
roughness = 0.9
height_mode = "parallax"
height_scale = 0.03

[textures]
occlusion = "brick.ao.jpg"
```
