# assetc-noise

The procedural noise of **[assetc](https://github.com/qbart/assetc)**, the asset compiler, as a crate of its own: pick a generator, set its parameters, add color modifiers, and get the same tileable texture assetc builds from an `.anoise` file, to the pixel.

## What it holds

- **`GeneratorKind`**: eleven generators: checkerboard, uniform, value, Perlin, simplex, fBm, Voronoi, Perlin-Worley, Musgrave, ridged and curl. Each lists its parameters (`specs`): keys, labels, ranges and defaults (seed, basis noise, separate X / Y frequency, translation, octaves, lacunarity, gain, domain warp, and each generator's extras).
- **`Noise`**: a generator, its parameters, the image size and a list of color **modifiers** (levels, brightness and contrast, threshold, posterize, blur, normalize, colorize, …); `generate` makes the RGBA8 image. Every noise tiles seamlessly, and the same parameters always give the same pixels.
- **`set_kind`**: switches generators while keeping each one's parameters, as an editor would.

Images, parameters and modifiers are [`assetc-colors`](https://crates.io/crates/assetc-colors)' types, re-exported. There are no file formats: reading and writing `.anoise` files stays in assetc; the exact algorithms are in [`docs/spec/anoise.md`](https://github.com/qbart/assetc/blob/main/docs/spec/anoise.md) and this crate's page is [`docs/features/noise-crate.md`](https://github.com/qbart/assetc/blob/main/docs/features/noise-crate.md).

## Usage

```toml
[dependencies]
assetc-noise = "0.5"
```

```rust
use assetc_noise::{GeneratorKind, Modifier, ModifierKind, Noise, Value};

// A 256 px tileable fBm, with 6 octaves, inverted.
let mut noise = Noise::new(GeneratorKind::Fbm);
noise.size = 256;
let specs = noise.kind.specs();
let octaves = specs.iter().position(|s| s.key == "octaves").unwrap();
noise.params.0[octaves] = Value::Int(6);
noise.modifiers.push(Modifier::new(ModifierKind::Invert));

let image = noise.generate();
assert_eq!(image.dimensions(), (256, 256));
assert!(!noise.has_color());
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
