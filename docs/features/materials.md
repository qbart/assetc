# Materials

PBR materials in the glTF 2.0 **metallic-roughness** model. A material is
built from texture files that follow a naming convention, an optional
`<name>.mat.toml` holding its scalar values and overrides, or both. Each
material compiles to one `.mat` file (see [`docs/spec/material.md`](../spec/material.md))
that references its textures by asset ID.

## Slots

| Slot         | Suffix | Color space | Compiled to                         |
|--------------|--------|-------------|-------------------------------------|
| `base_color` | none   | sRGB        | its own texture                     |
| `normal`     | `.n`   | linear      | its own texture                     |
| `roughness`  | `.r`   | linear      | packed into ORM, channel G          |
| `metallic`   | `.m`   | linear      | packed into ORM, channel B          |
| `occlusion`  | `.ao`  | linear      | packed into ORM, channel R          |
| `emissive`   | `.e`   | sRGB        | its own texture                     |
| `height`     | `.h`   | linear      | its own texture                     |

Suffixes are case-sensitive. Alpha comes from the base color texture's alpha
channel; there is no separate opacity slot yet.

## Finding materials

Texture sources (`*.png`, `*.jpg`, `*.jpeg`, see [`build.md`](build.md#inputs))
and material files (`*.mat.toml`) are grouped per directory by **name**:

- `<name>.<suffix>.<ext>` with a suffix from the table joins material `<name>`
  in that slot, e.g. `brick.n.jpg` → material `brick`, slot `normal`.
- `<name>.mat.toml` belongs to material `<name>`.
- `<name>.<ext>` (no suffix) is the **base color** of material `<name>`, if
  that material exists.

A material exists when its directory has a `<name>.mat.toml` or at least one
suffixed slot file. A lone `grass.png` stays a plain texture, and so does
`a.b.png` when `b` is not a suffix.

```
samples/brick/
  brick.jpg       base_color   ┐
  brick.n.jpg     normal       │
  brick.r.jpg     roughness    ├ material brick/brick.mat
  brick.ao.jpg    occlusion    │
  brick.h.jpg     height       │
  brick.mat.toml  (optional)   ┘
```

### `<name>.mat.toml`

Holds the scalar values and can reassign slots; see
[`docs/spec/mat.toml.md`](../spec/mat.toml.md) for every key. Example:

```toml
roughness = 0.9            # multiplies the roughness map
normal_y = "dx"            # normal map made for DirectX (green down)
height_scale = 0.03

[textures]
occlusion = "../shared/ao.png"
```

- A key under `[textures]` replaces the convention file for that slot. The
  replaced file is then a plain texture again.
- A `.mat.toml` without any textures is a valid, scalar-only material.
- Factors follow glTF: the shader multiplies a texture by its factor, and a
  missing texture counts as 1.

## Textures used by materials

- A texture in a material slot is compiled in the slot's color space and
  with mipmaps unless `no_mipmaps` matches it. The `[textures] linear`
  globs do not apply to it.
- The same file may be used by several materials in the same color space.
  Using it in both an sRGB and a linear slot is an error.
- `normal_y = "dx"` flips the green channel of the normal texture while it is
  compiled, so every compiled normal map is OpenGL / glTF style (+Y up). A file
  used as a normal map by two materials with different `normal_y` is an
  error.
- Slot textures keep the asset path they would have as plain textures
  (`brick/brick.n`).
- Using a file as an ORM input does not produce a texture from that file by
  itself.

### ORM packing

If a material has any of `occlusion`, `roughness` or `metallic`, they are
packed into one linear texture `<dir>/<name>.orm` (`<name>.orm.ktx2`), with
a full mip chain:

| Channel | Content                                   | When the slot has no file |
|---------|-------------------------------------------|---------------------------|
| R       | occlusion                                 | 255                       |
| G       | roughness                                 | 255                       |
| B       | metallic                                  | 255                       |
| A       | 255                                       | 255                       |

- Each input's **red** channel is used, so grayscale files (or grayscale saved
  as RGB) work as-is.
- All ORM inputs of one material must have the same dimensions, otherwise the
  material fails.
- A missing slot is filled with 255 so its factor alone applies: no
  occlusion, and roughness / metallic equal to their factors.

## Defaults

| Value                | Default                                     |
|----------------------|---------------------------------------------|
| `base_color`         | `[1, 1, 1, 1]` (linear RGBA)                |
| `metallic`           | `1.0` with a metallic map, otherwise `0.0`  |
| `roughness`          | `1.0`                                       |
| `occlusion_strength` | `1.0`                                       |
| `normal_scale`       | `1.0`                                       |
| `normal_y`           | `"gl"`                                      |
| `emissive`           | `[0, 0, 0]`, or `[1, 1, 1]` with an emissive map |
| `emissive_strength`  | `1.0`                                       |
| `height_mode`        | `"parallax"`                                |
| `height_scale`       | `0.05`                                      |
| `height_midlevel`    | `0.5`                                       |
| `alpha_mode`         | `"opaque"`                                  |
| `alpha_cutoff`       | `0.5`                                       |
| `double_sided`       | `false`                                     |

Height is stored as height: white is high. Engines that expect a depth map
(e.g. Bevy's `depth_map`) use `1 - height`.

## IDs and outputs

- **Material asset path**: `<dir>/<name>.mat`, e.g. `brick/brick.mat`, and
  ID = FNV-1a-64 of it, as for every asset. The `.mat` keeps it distinct from
  the base color texture `brick/brick`.
- **Output**: `<output>/<dir>/<name>.mat`. The file name is the asset path
  itself, since it already ends in `.mat`.
- Manifest entries: one `material` per material, plus one `texture` per
  compiled slot texture and per ORM texture.
- Asset path collisions (e.g. a source `brick.orm.png` next to a material
  `brick`) are build errors, as for any assets.

## Failures

The material fails, and with it the build (no manifest is written), when:

- its `.mat.toml` does not parse, has unknown keys, or has values out of range;
- a `[textures]` path does not exist, is not a texture source, or leaves
  `input`;
- ORM inputs differ in size;
- a texture is used in conflicting color spaces or with conflicting
  `normal_y`;
- a slot texture fails to decode.

## CLI

```
brick/brick.jpg -> brick/brick.ktx2 (srgb, 11 levels)
brick/brick.n.jpg -> brick/brick.n.ktx2 (linear, 11 levels)
brick/brick.ao.jpg, brick/brick.r.jpg -> brick/brick.orm.ktx2 (linear, 11 levels)
brick/brick.* -> brick/brick.mat (material: base_color, normal, orm, height)
Built 4 textures, 1 material (3 skipped)
```

The material line's source is `<dir>/<name>.mat.toml` when that file exists,
otherwise `<dir>/<name>.*`.

## Inspector

- **Material source** (Input `*.mat.toml`): Material (asset path), Asset ID,
  Output (with `(not built)` when missing).
- **Source texture** that belongs to a material gets a **Material** row,
  e.g. `brick/brick.mat (roughness)`. Its **Color** row is the slot's color space;
  for ORM inputs, **Output** shows the ORM texture, e.g.
  `brick/brick.orm.ktx2 (channel G)`.
- **Compiled material** (Output `*.mat`): every value from the `.mat` file,
  and each texture slot's asset ID (or `—`).
- The Assets tab is enabled for `.mat` files too and lists the material.

## Keybindings

None.
