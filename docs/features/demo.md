# Demo

`assetc-demo` is a small Bevy game-like app that shows **every sample** the
way a game would: it opens the pack `assetc pack` writes and turns each asset
into Bevy resources through the [SDK](sdk.md) only. It does not use assetc's
own code, the source files in `samples/` or any loader crate (no `image`,
`gltf` or KTX2 crate); if the demo can show it, a game can load it.

## Running

```
$ cargo run --release -p assetc-demo -- [path]
```

- `path`: a `.pak` file or an output directory (see
  [`Assets::open`](sdk.md#opening-assets)); default `tmp/build/assetc.pak`,
  the pack of this repository's `assetc.toml`.
- When it cannot open the assets, the window shows the error and
  `Run: cargo run -- pack` instead of the scenes.
- Lives in `demo/`, a member of the Cargo workspace. Its only dependencies
  are `assetc-sdk` and `bevy`.

## Layout

A full window: a bar of scene tabs along the top, the scene below it, and a
status line at the bottom (`<n> assets from <path>` and the scene's hint).
Colors follow the GUI's look (dark background, `#F0883E` accent).

## Scenes

| # | Scene | Shows |
|--:|-------|-------|
| 1 | **Textures** | Every 2D texture (not cubemaps or volumes) as a tile in a grid, with its path below; mip level 0. |
| 2 | **Materials** | Every material (`*.amat.bin`) on a sphere with its textures (base color, normal, ORM, emissive, depth from height) and values, lit by the sky (see below), in rows with their path below. |
| 3 | **Models** | Every mesh (`*.amesh.bin`) with its materials, side by side. A skinned mesh plays the clips of its `.aanim.bin` (same path, `.aanim.bin` for `.amesh.bin`); its locators are drawn as small axes with their names. |
| 4 | **Atlases** | Every atlas: all frames laid out with their names; a frame list that looks like a flipbook (names ending in a number) also plays as an animation at 12 frames per second. |
| 5 | **Fonts** | Every font: a pangram, the digits and the font's path drawn with its SDF atlas at three sizes, using the SDK's line layout and kerning. |
| 6 | **Data** | Every data file: each instance with its type, id and fields (nested values indented), as text. |
| 7 | **Graphs** | Every directed graph and tree: nodes at their positions as rounded boxes with their labels and colors, edges as arrows (a tree: lines from parent to child). |

The **sky** is the pack's cubemap textures (`*.ktx2` with 6 faces, the
first by path): it is the 3D scenes' skybox and environment light. Without
one, the 3D scenes use a plain background and a directional light.

## Controls

| Input | Action |
|-------|--------|
| `1`–`7`, click on a tab | Show that scene |
| `Tab` / `Shift+Tab` | Next / previous scene |
| Left drag (3D scenes) | Orbit the camera |
| Scroll wheel | 3D scenes: move closer / farther; 2D scenes: scroll |
| `Space` (Models) | Next animation clip |
| `Escape` | Quit |

## What it shows a game developer

The demo's source is the reference for using the SDK with Bevy, one module
per scene: textures to `Image` (`Rgba8UnormSrgb`, `Rgba8Unorm`,
`Rgba16Float`, cubemaps), materials to `StandardMaterial`, meshes to `Mesh`
with joints as `SkinnedMesh` entities driven by sampled animation channels,
atlases and fonts to sprites, and data, graphs and trees read as plain
values.
