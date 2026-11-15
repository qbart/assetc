# assetc-colors

The color logic of **[assetc](https://github.com/qbart/assetc)**, the asset compiler, as a crate of its own: use the same gradients, sRGB conversion, color modifiers, LUTs and color vision simulation as assetc, with the same results to the byte.

## What it holds

- **`space`**: sRGB and linear light (`srgb_to_linear`, `linear_to_srgb`, byte versions), HSV and HSL, luminance, and `#rrggbb` / `#rrggbbaa` hex colors.
- **`gradient`**: gradients as stops, blended premultiplied (fading to transparent keeps the color), with Bayer dithering, and their images.
- **`grid`**: an image split into cells filled in row or column order, as gradient and palette images are.
- **`modifier`**: color modifiers (invert, levels, brightness and contrast, threshold, posterize, blur, normalize, colorize) with their parameters.
- **`blend`**: blend modes (mix, multiply, screen, overlay, add).
- **`lut`**: 3D color LUTs from `.cube` files: apply to a color, grade an image, or lay out as a strip or a volume.
- **`vision`**: color vision deficiency simulation (protanopia, deuteranopia, tritanopia, monochromacy).
- **`heat`**: a weight as a heatmap color.

It depends on nothing but `std`: colors are `[u8; 4]` or floats, images are RGBA8 bytes ([`Image`]). Reading and writing assetc's files stays in assetc; the format specs are in [`docs/spec/`](https://github.com/qbart/assetc/tree/main/docs/spec) and this crate's page is [`docs/features/colors.md`](https://github.com/qbart/assetc/blob/main/docs/features/colors.md).

## Usage

```toml
[dependencies]
assetc-colors = "0.2"
```

```rust
use assetc_colors::gradient::{self, Dither, Stop};
use assetc_colors::space::{format_hex, parse_hex, srgb_byte_to_linear};

let stops = [
    Stop { at: 0.0, color: parse_hex("#1b1036").unwrap() },
    Stop { at: 1.0, color: parse_hex("#ffe66d").unwrap() },
];
// The middle of the gradient, and a 16 × 64 RGBA8 cell of it, dithered.
let middle = gradient::color_at(&stops, 0.5);
let cell = gradient::render(&stops, Dither::Crosshatch, 16, 64);
assert_eq!(cell.len(), 16 * 64 * 4);
println!("{} is {:.3} red in linear light", format_hex(middle), srgb_byte_to_linear(middle[0]));
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
