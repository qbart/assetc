# Noise crate

assetc's noise generators, the part that turns an [`.anoise`](noise.md)
file's generator, parameters and modifiers into an image, are their own
crate, **`assetc-noise`**, so other programs (editors, tools, a game making
textures at run time) generate exactly the textures assetc builds: the same
generators, the same parameters and defaults, the same pixels.

## Crate

- Package `assetc-noise`, library `assetc_noise`
  (`use assetc_noise::{GeneratorKind, Noise};`). It lives in `noise/`, a
  member of the Cargo workspace, and is published to
  [crates.io](https://crates.io/crates/assetc-noise).
- Depends only on [`assetc-colors`](colors.md): its `Image`, and the
  parameters and color modifiers (`ParamSpec`, `Params`, `Value`,
  `Modifier`, `ModifierKind`, `apply_modifiers`), all re-exported.
- No file formats: no `.anoise` text, no TOML. It works on plain values: a
  `Noise` in, an RGBA8 image out. Reading and writing `.anoise` files (and
  the modifier tables of `.amod` and `.agradient` files) stay in the
  compiler.
- The version is the workspace's, set in `noise/Cargo.toml` with the
  others (see [Version](release.md#version)).
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`noise/LICENSE-MIT`, `noise/LICENSE-APACHE`). Needs Rust 1.88 or newer
  (`rust-version`).
- `noise/README.md` is both its crates.io page and the front page of its
  API docs; its example is compiled as a doctest.
- The compiler (`assetc-core`) depends on it by path and generates every
  `.anoise` texture with it, in the build, the canvas preview and the
  Inspector: what the crate holds is not also kept in the compiler. The
  compiler keeps the `.anoise` parser and writer, the modifier tables'
  parser and writer, the Parameters and Modifiers tabs and creating files.
- The [SDK](sdk.md) does not depend on it; it reads the built
  `.atex.bin` files.

```toml
[dependencies]
assetc-noise = "0.4"
```

## Contents

| Item | Holds |
|------|-------|
| `Image`, `Modifier`, `ModifierKind`, `ParamKind`, `ParamSpec`, `Params`, `Value`, `apply_modifiers` | From `assetc-colors`, re-exported. |
| `GeneratorKind` | The eleven [generators](noise.md#generators); `ALL` (in menu order), `name` (`fBm`), `key` (`noise/fbm`, the file's `type`), `from_key`, `file_stem` (`fbm`), `section` (`noise.fbm`, the table holding its parameters), `index` (its place in `ALL`) and `specs`: its parameters, with their keys, labels, ranges and defaults, in [table order](../spec/anoise.md#generators). |
| `MIN_SIZE`, `MAX_SIZE` | The image size range, 1 to 4096 px. |
| `Noise` | An `.anoise` file's content: `kind`, `size`, `params` (of `kind`), `kept` (other generators' parameters, in `ALL` order) and `modifiers`. `Default` and `new(kind)`: size 512, the generator's defaults, no modifiers. `set_kind`: switches generators, keeping the current parameters and restoring the new generator's (not keeping all-default ones), as the Parameters tab does. `has_color`: whether the image has color (Curl, or an enabled Colorize). `generate`: the [image](../spec/anoise.md#generated-image), `size` × `size` RGBA8, opaque. |
| `splitmix64` | The hash every generator is built on ([SplitMix64](../spec/anoise.md#hash)), for tools of their own. |

`generate` takes the parameters as they are: values out of their ranges
are not checked (a caller building `Params` by hand keeps them in range;
the compiler's parser rejects the rest).

## Publishing

`make publish` publishes `assetc-noise` after `assetc-colors` (which it
depends on) and before the SDK; see [Publishing](colors.md#publishing).
