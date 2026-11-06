# assetc

Reads the files the assetc compiler builds and packs:
textures (KTX2), materials, meshes, animations, fonts, atlases, gradients,
palettes, shaders and data. It depends on nothing but `std` and returns
plain Rust values, so any engine can turn them into its own resources.

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
