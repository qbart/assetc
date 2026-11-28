# assetc-collider

The convex decomposition of **[assetc](https://github.com/qbart/assetc)**, the asset compiler, as a crate of its own: give it a model's triangles, get back a few convex hulls that together follow its shape (a compound collider), made exactly as assetc makes `.acollider` colliders.

Physics engines (Jolt, PhysX, Rapier, Bullet, Havok, …) collide convex shapes fast, but a chair, a rock arch or a car is not convex: one hull around all of it fills the space under the chair's seat. Like V-HACD and CoACD, this splits the model into pieces that are each close to convex and wraps each in its own hull.

## What it holds

- **`decompose`**: triangles into hulls. The model is voxelized (filled inside when it is closed), cut along planes where its hulls would hold the most empty space, each piece's hull is made around the model's own surface inside it (so it fits the faces, not the voxel steps), and pieces are merged back while they stay within the error.
- **`Settings`**: the most hulls, the most corner points per hull, the error (empty space the hulls may hold, as a share of the model's volume) and the voxel resolution, with their defaults and ranges, and `validate`.
- **`Collider`** and **`Convex`**: the hulls, largest first, each with its volume, center and box.
- **`hull`** and **`Hull`**: the 3D convex hull of any points (quickhull), its faces, edges and volume.

Points are plain `[f32; 3]` arrays and triangles `[u32; 3]` indices, so it works with any math crate or engine. There are no file formats: reading glTF models and writing assetc's `.acollider.bin` stay in assetc; this crate's page is [`docs/features/collider-crate.md`](https://github.com/qbart/assetc/blob/main/docs/features/collider-crate.md).

## Usage

```toml
[dependencies]
assetc-collider = "0.4"
```

```rust
use assetc_collider::{decompose, Settings};

// An L of two boxes: 2 × 1 × 1, and 1 × 2 × 1 on top of its left end.
let mut positions = Vec::new();
let mut triangles = Vec::new();
for (min, max) in [([0.0, 0.0, 0.0], [2.0, 1.0, 1.0]), ([0.0, 1.0, 0.0], [1.0, 3.0, 1.0])] {
    let base = positions.len() as u32;
    for i in 0..8 {
        positions.push([0, 1, 2].map(|k| if i >> k & 1 == 0 { min[k] } else { max[k] }));
    }
    for t in [[0, 2, 1], [1, 2, 3], [4, 5, 6], [5, 7, 6], [0, 1, 4], [1, 5, 4],
              [2, 6, 3], [3, 6, 7], [0, 4, 2], [2, 4, 6], [1, 3, 5], [3, 7, 5]] {
        triangles.push(t.map(|i| base + i));
    }
}

let collider = decompose(&positions, &triangles, &Settings::default())?;
// Two hulls, close to the L's own volume (4) rather than its box's (6).
assert_eq!(collider.hulls.len(), 2);
assert!((collider.volume() - 4.0).abs() < 0.25);
for convex in &collider.hulls {
    println!("{} points, center {:?}", convex.hull.points.len(), convex.center);
}
# Ok::<(), String>(())
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.
