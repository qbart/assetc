# assetc

The runtime SDK of **[assetc](https://github.com/qbart/assetc)**, the asset compiler. Learn more about the project, its GUI and every source and output format in the [main README](../README.md).

## The project

`assetc` compiles source assets (textures, meshes, shaders, materials, fonts) into runtime-friendly binary formats for video games. Supports octahedral-packed normals/tangents and meshletized geometry; mesh files reference materials by stable 64-bit hashes so the engine can share resources across assets. Mesh additionally emits a companion material table plus one content-addressed .ktx2 per referenced image. Embedded textures are written to a shared flat store keyed by content, so the same texture used by two different models is encoded once and shares one runtime handle.

Texture refs are stored as 64-bit hashes, not paths. A single global manifest maps each hash back to the file on disk. Load it once into a hash → path map and you can resolve any asset by a stable id - textures (baseColorTex etc.), but also meshes, materials, animations, fonts, shaders, and embeds - through one uniform lookup.

## What the SDK does

assetc runs at build time; this crate runs in your game. It reads everything `assetc build` and `assetc pack` write, so the game never parses a binary layout itself:

- **Opens the assets** from a `.pak` (the whole output in one file) or from an output directory, the same way, so you can ship a pack and develop against loose files.
- **Lists and finds them** by kind, by asset ID (the stable 64-bit hash every asset and reference uses) or by output path.
- **Decodes every format** into plain Rust values: KTX2 textures with all mip levels, PBR and custom materials, meshes (vertices with unpacked normals and tangents, meshlets, levels of detail, skin, locators, shapes, regions, paths), animations (with sampling), SDF fonts (with text layout), atlases, gradients (with sampling), palettes, shader interfaces with SPIR-V, schemas, data instances, graphs, trees and attached structs.
- **Stays engine-neutral and light**: no dependencies but `std`. Turning the values into GPU resources is your engine's part; the repository's demo does it for Bevy.

The format specs it follows are in [`docs/spec/`](../docs/spec/), and its API in [`docs/features/sdk.md`](../docs/features/sdk.md).

## Usage

```toml
[dependencies]
assetc = "0.1"
```

```rust,no_run
use assetc::{Assets, Kind, Mesh, Texture};

fn main() -> assetc::Result<()> {
    // A `.pak` from `assetc pack`, or the output directory of `assetc build`.
    let assets = Assets::open("build/assetc.pak")?;
    for entry in assets.of_kind(Kind::Mesh) {
        let mesh: Mesh = assets.load(entry.id)?;
        println!("{}: {} vertices", entry.path, mesh.vertices.len());
    }
    let texture: Texture = assets.load_path("ui/fire.ktx2")?;
    println!("{}x{}", texture.width, texture.height);
    Ok(())
}
```

Licensed under either of MIT ([LICENSE-MIT](LICENSE-MIT)) or Apache-2.0
([LICENSE-APACHE](LICENSE-APACHE)), at your option.
