# assetc-atlas

The atlas packing of **[assetc](https://github.com/qbart/assetc)**, the asset compiler, as a crate of its own: give it named images, get back one atlas image and where each frame is, laid out exactly as assetc lays out `*.atlas` folders.

## What it holds

- **`pack`**: named images into one atlas, either as a **grid** (every frame in a cell of one size, row by row: flipbooks; frames larger than their cell are scaled down to fit) or **packed** tightly on shelves, optionally **trimmed** to their opaque pixels. Frames get **padding** around them, their edges can be **extruded** into it so neighbours never bleed when sampled or mipmapped, and the atlas can be rounded to **powers of two** within a **max size**.
- **`Settings`** and **`Packing`**: those choices, their defaults and ranges, and `validate`.
- **`Atlas`** and **`Frame`**: the result: the image, and each frame's rectangle in it, the size of the image it came from and, when trimmed, where it started there; texture coordinates with `uv`.
- **`trim_bounds`** and **`blit`**: the two image operations packing is made of, for tools of your own.

Images are RGBA8 ([`Image`], from [`assetc-colors`](https://crates.io/crates/assetc-colors)). There are no file formats: reading image files, assetc's `atlas.toml` and writing `.aatlas.bin` stay in assetc; the format specs are in [`docs/spec/`](https://github.com/qbart/assetc/tree/main/docs/spec) and this crate's page is [`docs/features/atlas-crate.md`](https://github.com/qbart/assetc/blob/main/docs/features/atlas-crate.md).

## Usage

```toml
[dependencies]
assetc-atlas = "0.4"
```

```rust
use assetc_atlas::{pack, Image, Packing, Settings};

// Three sprites, trimmed and packed tightly with 1 px of extruded edge.
let sprites = vec![
    ("coin".to_string(), Image::filled(16, 16, [255, 200, 0, 255])),
    ("gem".to_string(), Image::filled(12, 20, [40, 120, 255, 255])),
    ("heart".to_string(), Image::filled(18, 16, [230, 30, 60, 255])),
];
let settings = Settings {
    packing: Packing::Pack,
    trim: true,
    extrude: 1,
    power_of_two: true,
    ..Settings::default()
};
let atlas = pack(&settings, sprites)?;
let (width, height) = atlas.image.dimensions();
let gem = atlas.frame("gem").unwrap();
println!("{width} × {height} px, gem at ({}, {}) uv {:?}", gem.x, gem.y, gem.uv(width, height));
# Ok::<(), String>(())
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
