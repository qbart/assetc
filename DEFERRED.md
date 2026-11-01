# Deferred

Ideas checked and parked for later, with what is known so far.

## `.exr` environment maps → KTX2 cubemap

Use the [`exr`](https://docs.rs/exr/latest/exr/) crate (1.74.2, BSD-3-Clause,
pure Rust, `#[forbid(unsafe_code)]`) to read OpenEXR HDR images and compile
them into a KTX2 cubemap.

What the crate covers (from its docs):

- Reads RGBA and multi-layer / multi-part images with `f16`, `f32` or `u32`
  samples (`read_first_rgba_layer_from_file`, `read_all_rgba_layers_from_file`).
- Reads mip / rip map levels of an image; does not generate mip maps.
- Decodes DWAA / DWAB (encoding only is missing); HTJ2K is unsupported; deep
  data is unsupported (reading fails if a layer has deep data).

What assetc would have to do itself:

- **Cube layout**: the docs do not mention cube maps, environment maps or
  OpenEXR's `envmap` attribute (the `meta::attribute` module was not checked).
  Either accept six face files (like `*.env` folders, with `px.exr` …) or a
  latitude-longitude image and resample it to six faces.
- **HDR output**: a KTX2 float format (e.g. `R16G16B16A16_SFLOAT`) and its
  spec in `docs/spec/ktx2.md`; today assetc only writes 8-bit RGBA.
- **Mip chain** (and, for lighting, prefiltered specular / diffuse maps) — the
  crate leaves this to the caller.
- **Preview**: the 3D environment view currently uploads 8-bit sRGB faces.

## Modifiers on input images

Let ordinary texture sources (`.png`, `.jpg`) carry the same modifier stack
as `.noise` files, so the build exports the modified image.

Plan so far:

- **Storage**: an optional sidecar next to the image,
  `<file name>.modifiers.toml` (e.g. `brick.jpg.modifiers.toml`), holding
  only `[[modifiers]]` tables in the `.noise` format (spec in
  `docs/spec/modifiers.md`).
- **Editing**: the Inspector's Modifiers tab is enabled for `.png` / `.jpg`
  files in the Input tree; adding the first modifier creates the sidecar,
  removing the last deletes it.
- **File operations**: renaming or deleting an image renames / deletes its
  sidecar too.
- **Build and preview**: every place that loads a texture source
  (`build::load_source`: plain textures, material slots, ORM packing, the
  canvas) applies the enabled modifiers in order, so the preview is what is
  exported. A grayscale image becomes RGB when an enabled `colorize` is
  present. Modifiers work on stored 8-bit values / 255 (sRGB images are not
  linearized); alpha is unchanged.
- **Needed in code**: modifiers become their own module, shared by noise and
  images, and `blur` must handle non-square images (it assumes `size × size`
  today). Environment-map faces (`*.env`) are left out.
- **Open**: whether blur should wrap at the edges (fine for tiling textures)
  or clamp for non-tiling images, perhaps as a modifier parameter.

## `.obj` models

Preview (and later compile) Wavefront OBJ files like glTF models (see
`docs/features/models.md`). Nothing in the dependency tree reads OBJ: it
needs a crate such as `tobj` or a small parser of our own (positions,
normals, UVs, faces; `.mtl` materials optional). The file tree already shows
`.obj` files with the `torus` icon.

## Compiling 3D files (`.amesh`, `.amat`, `.a<format>`)

Compile glTF models (and later `.obj`) into runtime binaries, with
[assetc-cpp](https://github.com/qbart/assetc-cpp) as the reference for what
the output holds. Today models are only inspected and previewed (see
`docs/features/models.md`); the build ignores them.

What the output should cover:

- **Meshes**: vertex streams (positions, normals, tangents, UVs, colors,
  joints / weights), index buffers, one entry per primitive or submesh.
- **Bounds**: per mesh (AABB and sphere), per meshlet, and for the model.
- **Meshlets**: meshes split into meshlets (vertex / triangle lists, bounds,
  cone data for culling).
- **Materials**: the glTF materials, compiled like `*.mat.toml` materials,
  with their textures compiled as KTX2 and referenced by asset ID.
- **Skins**: joints, hierarchy and inverse bind matrices.
- **Animations**: clips with their channels and keyframes.

Naming: assetc's own binary formats use `.a` + the format name (`a` for
"assetc"): `.amesh` for meshes, `.amat` for materials, and `.a<format>` for
each further format (e.g. animations and skins). Each one gets a spec in
`docs/spec/` and its own manifest kind, with every asset getting a hashed ID.

Open:

- Whether the existing compiled material format `.mat` is renamed to `.amat`
  (with a spec and manifest update), so all assetc formats follow the scheme.
  `.ktx2` stays as it is, since it is a standard format.
- How one glTF splits into files: one file per mesh / material / animation,
  or one container per model with several assets (the Inspector's Assets tab
  already expects containers to list several).
- Meshlet parameters (vertex and triangle limits) and whether they belong in
  `assetc.toml`.

## Modifiers for gradients

Let `.gradient` files carry modifiers like `.noise` files (Invert, Levels,
Brightness / Contrast, Threshold, Posterize, Blur, Normalize, Colorize), so
a gradient atlas can be adjusted without editing every stop.

Plan so far:

- **Storage**: `[[modifiers]]` tables in the `.gradient` file, in the
  `.noise` written form (spec in `docs/spec/gradient.md`).
- **Editing**: the Inspector's Modifiers tab is enabled for `.gradient`
  files too.
- **Applying**: after the cells are rendered, before the build and the
  canvas use the image, so the preview is what is exported. The gradient
  editor's bars keep showing the stops as authored.
- **Needed in code**: modifiers become their own module (shared with noise,
  and with modifiers on input images, see above), working on RGBA (noise
  modifiers assume a square grayscale-or-RGB image today), and `blur` must
  handle non-square images.

Open:

- Per file (the whole atlas) or per gradient (each cell on its own, in its
  card)? Per gradient fits the cards and keeps blur from bleeding between
  cells.
- Whether modifiers apply to alpha, and how blur treats cell edges (clamp
  inside the cell).
