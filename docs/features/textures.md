# Textures

How a texture source is turned into a `.ktx2` file
(see [`build.md`](build.md) for which sources are textures, and
[`docs/spec/ktx2.md`](../spec/ktx2.md) for the output format).

## Settings

Textures used by a [material](materials.md) take their color space from the
material slot instead of `linear`; `no_mipmaps` still applies to them.

Per-texture settings come from the `[textures]` table in
[`assetc.toml`](../spec/assetc.toml.md). Each setting is a list of glob
patterns matched against the source path relative to `input`, `/`-separated,
with its extension (e.g. `textures/rock_n.png`).

```toml
[textures]
linear = ["**/*_n.png", "**/*_orm.png"]
no_mipmaps = ["ui/**"]
```

| Setting      | Texture matching any pattern                 | Otherwise (default)        |
|--------------|----------------------------------------------|----------------------------|
| `linear`     | linear data (normal, roughness, masks, …)    | sRGB color                 |
| `no_mipmaps` | base level only                              | full mip chain             |
| `volume_luts` | color LUT (`*.cube`) as a 3D texture, see [`lut.md`](lut.md#build) | 2D strip |

Glob syntax:

- `*` matches any run of characters except `/`.
- `**` matches any number of whole path segments, including none
  (`**/*.png` matches `a.png` and `a/b/c.png`).
- `?` matches one character except `/`; `[abc]`, `[a-z]`, `[!a]` match a
  character class; `{a,b}` matches either alternative.
- Matching is case-sensitive.
- An invalid pattern fails the whole build before any source is compiled.

## Color space

- **sRGB** (default): the pixels are color. The texture is written as
  `R8G8B8A8_SRGB`, so the GPU decodes it to linear when sampling.
- **linear**: the pixels are data and are used as-is. The texture is written as
  `R8G8B8A8_UNORM`.

Alpha is always linear and straight (not premultiplied).

## Mipmaps

Unless the texture matches `no_mipmaps`, a full mip chain is written, down to
1×1.

- Level count: `floor(log2(max(width, height))) + 1`.
- Level `n` size: `max(1, floor(width / 2^n))` × `max(1, floor(height / 2^n))`.
- Each level is computed from the previous level (8-bit values) with a box
  filter: texel `(x, y)` of a `w × h` level made from a `W × H` level is the
  average of the source texels with
  `floor(x·W/w) ≤ sx < floor((x+1)·W/w)` and
  `floor(y·H/h) ≤ sy < floor((y+1)·H/h)`. For even sizes that is the usual
  2×2 average; odd sizes fold the extra row/column into the last texel.
- **sRGB** textures are averaged in linear light: R, G, B are decoded with the
  sRGB transfer function, averaged, and encoded back. So a 2×2 checker of black
  and white becomes `188`, not `128`.
- **linear** textures and all alpha channels are averaged as stored.
- Results are rounded to the nearest 8-bit value (halves round up).
- Normal maps are averaged like any other linear data; the vectors are not
  renormalized.

## Modifiers

A `.png` / `.jpg` / `.jpeg` / `.tga` in `input` can carry the same modifier stack as
a [`.noise`](noise.md#modifiers-tab) file, kept in its
[`.amod`](../spec/amod.md#modifiers) sidecar (`brick.jpg.amod`) and edited
in the Inspector's Modifiers tab.

- The enabled modifiers run, in order, every time assetc loads the image:
  the build (as a plain texture, in a material slot, packed into an ORM
  texture, as an environment-map face), the canvas and the Inspector. What
  the canvas shows is what is exported.
- They work on the stored 8-bit values divided by 255; sRGB images are not
  linearized first. Alpha is kept as it is.
- A grayscale image becomes RGB when an enabled `colorize` is present (the
  Inspector's channels follow).
- The source file itself is never changed.
- A sidecar that does not parse fails the image like an undecodable file
  (`brick.jpg.amod: <problem>`).
- **Modifiers tab**: for images in the Input tree it works exactly as for
  `.noise` files (see [`noise.md`](noise.md#modifiers-tab)), but saves to the
  sidecar: adding the first modifier creates `<file>.amod`, and removing the
  last deletes it (unless it holds other settings).

## Output

CLI lines show the settings used, for example:

```
textures/Grass.png -> textures/Grass.ktx2 (srgb, 10 levels)
textures/rock_n.png -> textures/rock_n.ktx2 (linear, 10 levels)
ui/button.png -> ui/button.ktx2 (srgb, 1 level)
```
