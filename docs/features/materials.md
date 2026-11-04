# Materials

PBR materials in the glTF 2.0 **metallic-roughness** model. A material is
built from texture files that follow a naming convention, an optional
`<name>.amat` holding its scalar values and overrides, or both. Each
material compiles to one `.amat.bin` file (see [`docs/spec/amat.bin.md`](../spec/amat.bin.md))
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

Texture sources (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`, see [`build.md`](build.md#inputs))
and material files (`*.amat`) are grouped per directory by **name**:

- `<name>.<suffix>.<ext>` with a suffix from the table joins material `<name>`
  in that slot, e.g. `brick.n.jpg` → material `brick`, slot `normal`.
- `<name>.amat` belongs to material `<name>`.
- `<name>.<ext>` (no suffix) is the **base color** of material `<name>`, if
  that material exists.

A material exists when its directory has a `<name>.amat` or at least one
suffixed slot file. A lone `grass.png` stays a plain texture, and so does
`a.b.png` when `b` is not a suffix.

```
samples/brick/
  brick.jpg       base_color   ┐
  brick.n.jpg     normal       │
  brick.r.jpg     roughness    ├ material brick/brick.amat.bin
  brick.ao.jpg    occlusion    │
  brick.h.jpg     height       │
  brick.amat      (optional)   ┘
```

### `<name>.amat`

Holds the scalar values and can reassign slots; see
[`docs/spec/amat.md`](../spec/amat.md) for every key. It is TOML, like
`.amod`; the `.amat` extension matches the other authored sources
(`.anoise`, `.agradient`, …) and the compiled `.amat.bin`. Example:

```toml
roughness = 0.9            # multiplies the roughness map
normal_y = "dx"            # normal map made for DirectX (green down)
height_scale = 0.03

[textures]
occlusion = "../shared/ao.png"
```

- A key under `[textures]` replaces the convention file for that slot. The
  replaced file is then a plain texture again.
- A `.amat` without any textures is a valid, scalar-only material.
- Factors follow glTF: the shader multiplies a texture by its factor, and a
  missing texture counts as 1.

### Old `.mat.toml` files

Materials used to be `<name>.mat.toml`. Such a file is no longer read: the
build fails on it with `materials are <name>.amat files now: rename
<name>.mat.toml to <name>.amat`. The contents need no change.

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

- **Material asset path**: `<dir>/<name>.amat.bin`, e.g. `brick/brick.amat.bin`, and
  ID = FNV-1a-64 of it, as for every asset. The `.amat.bin` keeps it distinct from
  the base color texture `brick/brick`.
- **Output**: `<output>/<dir>/<name>.amat.bin`. The file name is the asset path
  itself, since it already ends in `.amat.bin`.
- Manifest entries: one `material` per material, plus one `texture` per
  compiled slot texture and per ORM texture.
- Asset path collisions (e.g. a source `brick.orm.png` next to a material
  `brick`) are build errors, as for any assets.

## Failures

The material fails, and with it the build (no manifest is written), when:

- its `.amat` does not parse, has unknown keys, or has values out of range;
- a `[textures]` path does not exist, is not a texture source, or leaves
  `input`;
- ORM inputs differ in size;
- a texture is used in conflicting color spaces or with conflicting
  `normal_y`;
- a slot texture fails to decode;
- it is an old `<name>.mat.toml` (see [above](#old-mattoml-files)).

## CLI

```
brick/brick.jpg -> brick/brick.ktx2 (srgb, 11 levels)
brick/brick.n.jpg -> brick/brick.n.ktx2 (linear, 11 levels)
brick/brick.ao.jpg, brick/brick.r.jpg -> brick/brick.orm.ktx2 (linear, 11 levels)
brick/brick.* -> brick/brick.amat.bin (material: base_color, normal, orm, height)
Built 4 textures, 1 material (3 skipped)
```

The material line's source is `<dir>/<name>.amat` when that file exists,
otherwise `<dir>/<name>.*`.

## Creating

**New Material** in the Input tree's context menu (see
[`file-operations.md`](file-operations.md#context-menu)) creates
`material.amat` (or `material 2.amat`, …) in the target folder, selects it
and starts renaming it, like the other New items. The file holds:

```toml
base_color = [0.8, 0.8, 0.8, 1.0]
roughness = 0.5
```

Renaming it to the name of existing textures (e.g. `brick.amat` next to
`brick.jpg`, `brick.n.jpg`) makes those textures its slots.

## Inspector

- **Material source** (Input `*.amat`): the Info tab shows Asset path, Asset
  ID, Output (with `(not built)` when missing), and an Error row if the file
  is invalid. The **Material** tab edits it (see [Material tab](#material-tab))
  and the canvas previews it in 3D (see [3D preview](#3d-preview)).
- **Source texture** that belongs to a material gets a **Material** row,
  e.g. `brick/brick.amat.bin (roughness)`. Its **Color** row is the slot's color space;
  for ORM inputs, **Output** shows the ORM texture, e.g.
  `brick/brick.orm.ktx2 (channel G)`.
- **Compiled material** (Output `*.amat.bin`): every value from the `.amat.bin` file,
  one row each (Base color, Metallic, Roughness, Occlusion, Normal scale,
  Emissive, Height mode, Height scale, Height midlevel, Alpha, Double-sided),
  and each texture slot's asset ID (or `—`). The canvas shows the file's
  fields as a table, see [Material view](inspector.md#material-view).
- The Assets tab is enabled for `.amat.bin` files too and lists the material.

## Material tab

The Inspector tab **Material** (icon `gem`) is shown for `*.amat` files in
the Input tree. It edits every key of the file in the
[property editor](gui.md#property-editor) style: folding section cards, a
label column and a control per row. A file that cannot be read shows its
error instead (`Can't read brick.amat: …`) and nothing is edited.

Each row:

- **Label**: `ACCENT` when the file sets the key, `TEXT_WEAK` when the row
  shows the default. Hovering the label shows the key, e.g. `roughness`.
- **Control**: shows the file's value, or the default (from
  [Defaults](#defaults), with the material's maps counted). Changing it sets
  the key.
- **Reset** (`rotate-ccw`, tooltip `Reset to default`), only when the key is
  set: removes the key from the file.
- **Help**: one line of `TEXT_WEAK` 12 px text under the row, wrapped.

| Section    | Row         | Key                  | Control | Help |
|------------|-------------|----------------------|---------|------|
| Surface    | Base color  | `base_color`         | color swatch, opens the [color picker](gui.md#property-editor) with alpha | Multiplies the base color texture. Alpha is opacity, used by Mask and Blend. |
| Surface    | Metallic    | `metallic`           | slider 0–1, 2 decimals | 0 is non-metal (plastic, wood, stone), 1 is bare metal. Multiplies the metallic map. |
| Surface    | Roughness   | `roughness`          | slider 0–1, 2 decimals | 0 is mirror-smooth, 1 fully rough. Multiplies the roughness map. |
| Surface    | Double-sided | `double_sided`      | checkbox | Draw the back of faces too, e.g. leaves, cloth, glass panes. |
| Alpha      | Mode        | `alpha_mode`         | Opaque / Mask / Blend | Opaque ignores alpha. Mask drops pixels below the cutoff. Blend mixes pixels with what is behind them. |
| Alpha      | Cutoff      | `alpha_cutoff`       | slider 0–1, 2 decimals; only with Mask | Pixels with alpha below this are not drawn. |
| Normal     | Scale       | `normal_scale`       | number, step 0.01 | Strength of the normal map's bumps; 0 flattens them, negative turns them inside out. |
| Normal     | Y direction | `normal_y`           | OpenGL (+Y up) / DirectX (+Y down) | Which way the normal map's green channel points. Maps from DirectX tools (Unreal, Substance's DirectX preset) need DirectX. |
| Occlusion  | Strength    | `occlusion_strength` | slider 0–1, 2 decimals | How much the occlusion map darkens ambient light; 0 turns it off. |
| Emission   | Color       | `emissive`           | color swatch, no alpha | Light the surface gives off by itself. Multiplies the emissive map; black is none. |
| Emission   | Strength    | `emissive_strength`  | number ≥ 0, step 0.01 | Multiplies the color; above 1 makes it glow brighter than white, for bloom. |
| Height     | Mode        | `height_mode`        | Parallax / Displacement | Parallax fakes depth in the shader. Displacement moves the vertices and needs a dense mesh. |
| Height     | Scale       | `height_scale`       | number ≥ 0, step 0.001 | How far white is from black, in mesh units. |
| Height     | Midlevel    | `height_midlevel`    | slider 0–1, 2 decimals | The height map value that stays on the surface; darker goes in, lighter comes out. |

Colors are stored linear (see [`docs/spec/amat.md`](../spec/amat.md)); the
picker shows and edits them as sRGB, like everywhere else in the GUI.

### Textures

The last section, **Textures**, has one row per slot (Base color, Normal,
Roughness, Metallic, Occlusion, Emissive, Height) with the help line under
the section title: *Files named `<name>.<suffix>.<ext>` next to the material
fill their slot by name. Choose another texture to use it instead.*

- The control is a drop-down showing the slot's texture:
  - the `[textures]` path, when the file sets one (label `ACCENT`, with
    Reset);
  - else the convention file, e.g. `brick.n.jpg (by name)` in `TEXT_WEAK`;
  - else `None` in `TEXT_WEAK`.
- The list holds **By name** (`brick.n.jpg`, or `None` when there is no
  such file), which removes the slot's `[textures]` key, then every texture
  source (`*.png`, `*.jpg`, `*.jpeg`, `*.tga`) in `input`, as the path from
  the material's folder (`../shared/ao.png`), sorted. Choosing one writes it
  under `[textures]`.
- Hovering the label shows the slot's key and suffix, e.g.
  `normal, by name: <name>.n.<ext>`.

### Saving

Like the other editors (see [`noise.md`](noise.md#saving)), the file is
written once a value differs from the file and nothing is being dragged or
typed into, so sliders save when released. The editor writes the whole file:
only the keys that are set, in the order of
[`docs/spec/amat.md`](../spec/amat.md#schema), floats rounded to 4 decimals.
Comments and formatting in the file are not kept. The canvas and the Info
tab update from the saved file; the build picks the change up on the next
build.

## 3D preview

Selecting a `*.amat` file in the Input tree shows the material on a shape in
the canvas 3D view (orbit, zoom and reset as in
[`canvas.md`](canvas.md#3d-controls)); there is no 2D view.

- **Values**: the ones being edited, so the shape changes while a slider is
  dragged, before the file is saved.
- **Textures**: the slots' source files (by name and `[textures]`), as the
  build would use them: base color and emissive as sRGB, the others linear;
  the normal map's green flipped for `normal_y = "dx"`; occlusion, roughness
  and metallic packed into one ORM texture like the build
  ([ORM packing](#orm-packing)). Each texture is scaled down to at most
  2048 px on its longer side.
- **Height** is shown as parallax (Bevy's depth map is `1 - height`, scaled
  by `height_scale`), also in Displacement mode; the shape is not displaced.
- **Problems**: when a texture cannot be used (a missing or undecodable file,
  ORM inputs of different sizes), the preview leaves that slot out and the
  canvas shows `<slot>: <reason>` in `TEXT_WEAK` in its bottom-left corner,
  8 px from the edges, one line each. A file that cannot be read shows the default material.
- **Toolbar** (see [`canvas.md`](canvas.md#toolbar)): the shape group
  (Box, Sphere, Cylinder, Torus; Sphere by default, remembered for all
  materials during the session, apart from the image 3D view's shape) and
  the color vision group.
- **Lighting**: a drop-down in the canvas' top-left corner, 8 px from the
  edges, labeled with the current choice: **Studio** (the default light of
  the 3D view) or any HDR environment map (`*.exr`) in `input`, by input
  path. Choosing a map loads it in the background, like an inspection. An environment map lights the shape and shows as the
  background, as in [`hdr.md`](hdr.md#preview-and-inspector). The choice is
  kept for the session.
- The shape turns slowly about its vertical axis, as images do in 3D.

## Keybindings

None; the 3D preview uses the 3D canvas controls (see
[`KEYS.md`](../../KEYS.md)).
