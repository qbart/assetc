# Collider crate

assetc's convex decomposition, the part that turns a model's triangles into
the convex hulls of a [collider](colliders.md), and its convex hulls are
their own crate, **`assetc-collider`**, so other programs (editors, tools,
a game making colliders at run time) make exactly the colliders assetc
builds: the same voxels, cuts, hulls and merges, the same points.

## Crate

- Package `assetc-collider`, library `assetc_collider`
  (`use assetc_collider::{decompose, Settings};`). It lives in `collider/`,
  a member of the Cargo workspace, and is published to
  [crates.io](https://crates.io/crates/assetc-collider).
- Depends only on [`matma`](https://crates.io/crates/matma) (no default
  features), for its vector math inside. Its own types take and give plain
  arrays (`[f32; 3]` points, `[u32; 3]` triangles), so it works with any
  math crate or engine.
- No file formats: no glTF, no `.acollider` sidecar, no `.acollider.bin`.
  It works on plain values: positions and triangles in, hulls out. Reading
  models, the sidecar's parser and writer, and the `.acollider.bin` writer
  stay in the compiler.
- The version is the workspace's, set in `collider/Cargo.toml` with the
  others (see [Version](release.md#version)).
- Author Bartłomiej Wójtowicz; licensed `MIT OR Apache-2.0`
  (`collider/LICENSE-MIT`, `collider/LICENSE-APACHE`). Needs Rust 1.88 or
  newer (`rust-version`).
- `collider/README.md` is both its crates.io page and the front page of its
  API docs; its example is compiled as a doctest.
- The compiler (`assetc-core`) depends on it by path and makes every
  collider (in the build and the [preview](colliders.md#3d-view)) and every
  [convex hull shape](shapes.md#shape) with it: what the crate holds is not
  also kept in the compiler. The Collider tab takes its ranges from the
  crate.
- The [SDK](sdk.md) does not depend on it; it reads `.acollider.bin` files
  on its own.

```toml
[dependencies]
assetc-collider = "0.4"
```

## Contents

| Item | Holds |
|------|-------|
| `Hull` | A convex hull: `points` (its corners, `[f32; 3]`) and `faces` (triangles as indices into `points`, counter-clockwise seen from outside); `edges` (each edge once, as index pairs, sorted) and `volume`. |
| `hull(points)` | The convex hull of `points` (quickhull, in `f64` inside); `None` when they do not span a volume (fewer than four points, or all on one plane, or not finite). |
| `Settings` | The [collider settings](colliders.md#settings): `max_hulls`, `max_vertices`, `error`, `resolution`, with the defaults of that table (`Default`), and `validate`: the first value out of range (`max_hulls must be 1 to 256, not 300`). |
| `MAX_HULLS`, `MAX_VERTICES`, `ERROR`, `RESOLUTION` | The settings' ranges, inclusive: 1–256, 4–255, 0.001–0.5, 16–256. |
| `Convex` | One hull of a collider: `hull`, `volume`, `center` (of its volume), `min`, `max` (its box), in the model's space. |
| `Collider` | `hulls` (largest first) and `model_volume` (the model's volume as voxelized); `points`, `faces` and `volume` (summed over the hulls) and `bounds` (`[min, max]` around every hull, zeros without hulls). |
| `decompose(positions, triangles, settings)` | The collider of the triangles, as [How it is made](colliders.md#how-it-is-made) describes. Fails on invalid settings, a triangle naming a position that is not there (`a triangle uses a vertex the model does not have`) or nothing to voxelize (`the model has no triangles to make a collider of`). Uses every core for the cut and merge costs; the result does not depend on how many there are. |

## Publishing

`make publish` publishes `assetc-collider` (it depends on no other assetc
crate) before the SDK; see [Publishing](colors.md#publishing).
