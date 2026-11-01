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
