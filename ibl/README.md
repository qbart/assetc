# assetc-ibl

The image-based lighting of **[assetc](https://github.com/qbart/assetc)**, the asset compiler, as a crate of its own: give it an HDR environment in latitude-longitude layout (as HDRI sites publish them), get back the skybox cubemap and the prefiltered maps a PBR renderer lights with, exactly as assetc builds them from `.exr` files.

## What it holds

- **`Hdr`**: a linear RGBA float latitude-longitude image; `sample` it along any direction, or turn it into a **`cubemap`** with its mip chain (faces +X, −X, +Y, −Y, +Z, −Z, as Vulkan picks them).
- **`specular`**: the environment prefiltered for glossy reflections (the split-sum approximation of Karis): one level per roughness, GGX importance-sampled over a Hammersley sequence, each sample read at the mip matching its footprint, so bright suns give smooth lobes and not dots. An engine samples it at `lod = roughness × (levels − 1)`.
- **`irradiance`**: the diffuse light from every direction (cosine-weighted, divided by π, so ambient light is `albedo × texel`), and its 9 **spherical harmonics** coefficients for lighting without a cubemap lookup; **`sh_irradiance`** evaluates them.
- **`Level`**: one cubemap level, with `sample` across face edges.

No random seeds: the same image always gives the same floats, and the work is spread over every core. It depends on nothing but `std` and has no file formats: reading OpenEXR files and writing assetc's `.atex.bin` (BC6H or half floats) stay in assetc. The exact algorithms are in [`docs/features/hdr.md`](https://github.com/qbart/assetc/blob/main/docs/features/hdr.md) and this crate's page is [`docs/features/ibl-crate.md`](https://github.com/qbart/assetc/blob/main/docs/features/ibl-crate.md).

## Usage

```toml
[dependencies]
assetc-ibl = "0.5"
```

```rust
use assetc_ibl::{Hdr, face_size, irradiance, lighting_source, sh_irradiance, specular};

// A 64 × 32 sky: bright above the horizon, dark below.
let (width, height) = (64, 32);
let pixels = (0..width * height)
    .map(|i| if i / width < height / 2 { [2.0, 2.2, 2.5, 1.0] } else { [0.1, 0.08, 0.05, 1.0] })
    .collect();
let sky = Hdr { width, height, pixels };

let skybox = sky.cubemap(face_size(sky.height)); // 16 px faces, 5 levels
let source = lighting_source(&sky);
let glossy = specular(&source, 256, 64); // 16, 8 and 4 px faces: roughness 0, 0.5 and 1
let (diffuse, sh) = irradiance(&source, 8);

assert_eq!(skybox.len(), 5);
assert_eq!(glossy.iter().map(|l| l.size).collect::<Vec<_>>(), [16, 8, 4]);
assert_eq!(diffuse.size, 8);
// Looking up gets more light than looking down.
assert!(sh_irradiance(&sh, [0.0, 1.0, 0.0])[0] > sh_irradiance(&sh, [0.0, -1.0, 0.0])[0]);
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
