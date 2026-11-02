# Vertex colors

A glTF model's vertex colors are compiled into its
[`.amesh`](../spec/amesh.md#color-sets) as **color sets**: named buffers of
one RGBA color per vertex, alongside `VERTICES`. A mesh can have several
sets; a game picks one by the FNV-1a-64 hash of its name.

Color sets are meant to grow: today they come from the glTF's `COLOR_n`
attributes; later assetc will author its own (gradients over the model,
baked masks and the like), written as further sets with their own names in
the same chunks.

## From glTF

- Each `COLOR_n` attribute (`n` = 0, 1, …) on any triangle primitive of
  the model becomes the set named `COLOR_n`. Sets are written in order of
  `n`.
- Values are stored as **RGBA, 8 bits per channel, unsigned normalized,
  linear** (as glTF defines them; not sRGB-encoded): floats are clamped to
  0–1 and rounded to the nearest of 255 steps, 16-bit values rounded to 8
  bits, and RGB colors (`VEC3`) get alpha 255.
- A primitive without a set that another primitive has gets white
  (`255, 255, 255, 255`) for its vertices in that set, which leaves its
  material unchanged.
- Vertices are written in the same order as `VERTICES`, so vertex `v` of
  every set belongs to vertex `v` of the mesh. A primitive whose normals
  are generated flat (no `NORMAL`) has its vertices split per triangle;
  each copy keeps its source vertex's colors.
- Models without `COLOR_n` get no color set chunks.

## Inspector Info

- A model in the Input tree: a **Color sets** row after Textures: the
  names of the `COLOR_n` attributes its primitives have (`COLOR_0, COLOR_1`),
  or `none`.
- A compiled mesh: a **Color sets** row after Skin: the set names, or
  `none`.
- The [Chunks view](models.md#chunks-view) shows `COLORSET` and `COLORS`.

## Previews

- The model's [preview](models.md#preview) shows `COLOR_0` as Bevy's glTF
  loader does: multiplied with the base color.
- The [compiled mesh preview](models.md#compiled-mesh-preview)'s
  **Material** mode does the same with the `COLOR_0` set when the mesh has
  one.

## Not yet

- Color sets authored in assetc (gradients, masks), and a sidecar to keep
  them.
- A display mode showing any set on its own, and wider formats (16-bit or
  float channels; the record's `format` leaves room for them).
