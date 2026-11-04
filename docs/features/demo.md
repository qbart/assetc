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
- The SDK reads a pack and an output directory the same way, so the demo
  shows the same scenes from either: `tmp/build/assetc.pak` or the loose
  `*.bin` and `*.ktx2` files of `tmp/build`.
- `make demo` does both steps: packs the samples (`cargo run --release --
  pack`), then runs the demo on that pack. `make demo ASSETS=tmp/build`
  runs it on the output directory instead.
- Lives in `demo/`, a member of the Cargo workspace. Its only dependencies
  are `assetc-sdk` and `bevy`.

## Layout

A full window: a bar of scene tabs along the top, the scene below it, and a
status line at the bottom (`<n> assets from <path> (pack)` or
`(directory)`, by what was opened).
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

## Scrolling

A 2D scene (Textures, Atlases, Fonts, Data, Graphs) is laid out for the
window it was shown in and scrolls when it does not fit:

- Its **content** is the box around everything it draws (sprites, text and
  graph boxes) plus a 12 px margin. The view never leaves it: scrolling
  stops at the content's edges, so nothing can be scrolled out of sight.
- A **scrollbar** shows along the right edge (vertical) and above the
  status line (horizontal) whenever the content is taller or wider than
  the view, and hides otherwise. The thumb's length is the visible part of
  the content and its place is where the view is.
- Drag the thumb to scroll; click the track beside it to move one page
  toward the click. The thumb turns the accent color while hovered or
  dragged.
- Resizing the window lays the scene out again for the new size, from the
  top.

The 3D scenes (Materials, Models) have no scrollbars.

## Controls

| Input | Action |
|-------|--------|
| `1`–`7`, click on a tab | Show that scene |
| `Tab` / `Shift+Tab` | Next / previous scene |
| Left drag (3D scenes) | Orbit the camera |
| Scroll wheel | 3D scenes: move closer / farther; 2D scenes: scroll up / down |
| `Shift` + scroll wheel, horizontal wheel (2D scenes) | Scroll left / right |
| `PageUp` / `PageDown` (2D scenes) | Scroll one page up / down |
| `Home` / `End` (2D scenes) | Scroll to the top / bottom |
| Drag a scrollbar thumb, click its track (2D scenes) | Scroll; one page toward the click |
| `Space` (Models) | Next animation clip |
| `Escape` | Quit |

## What it shows a game developer

The demo's source is the reference for using the SDK with Bevy, one module
per scene: textures to `Image` (`Rgba8UnormSrgb`, `Rgba8Unorm`,
`Rgba16Float`, cubemaps), materials to `StandardMaterial`, meshes to `Mesh`
with joints as `SkinnedMesh` entities driven by sampled animation channels,
atlases and fonts to sprites, and data, graphs and trees read as plain
values.
