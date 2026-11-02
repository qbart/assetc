# GUI Shell

The application window is an egui shell drawn over Bevy. It provides the
frame that later features plug into; it has no asset functionality of its own.

The window is titled `assetc` and opens **maximized**; restoring it gives
the platform's default size.

## Layout

```
+--------------------------------------------------------------+
| ◆  File  Edit  View  Help                         (menu bar) |
+-------------+------------------------------+-----------------+
| Input|Output|                              | Inspector       |
|             |                              |-----------------|
|             |     canvas (preview of the   | ℹ |             |
|             |     selected file)           | ▤ |             |
|             |                              | ⚙ |             |
+-------------+------------------------------+-----------------+
| Ready                                     assetc 0.1.0       |
+--------------------------------------------------------------+
```

- **Menu bar** (top, fixed height): logo (see [Logo](#logo)) followed by the
  `File`, `Edit`, `View`, `Help` menus. See [Menus](#menus).
- **Left sidebar** (resizable, default width 260 px, min 200 px): file
  browser with two tabs in its header row, **Input** and **Output**, plus
  build (`hammer` icon, see [`build.md`](build.md#gui)) and rescan
  (`refresh-cw` icon) buttons on the right. See [File browser](#file-browser).
- **Right sidebar** (resizable, default width 320 px, min 220 px): the
  **Inspector** only, under an `Inspector` header row: a narrow vertical tab
  strip on the left with icon tabs (`Info`, `Parameters`, `Modifiers`,
  `Gradients`, `Colors`, `Animations`, `Mipmaps`, `Levels of detail`, `Assets`, see [`inspector.md`](inspector.md#tabs)) and the active tab's content on the right. Shows the selected
  file, see [`inspector.md`](inspector.md).
- **Status bar** (bottom, fixed height): status message on the left (a
  failed [file operation](file-operations.md#errors) shows there until the
  next operation or build), preceded by the **loader**: the `loader` icon
  (14 px, `ACCENT`) spinning one turn per second while background work
  runs (a [build](build.md#gui) or an [inspection](inspector.md#loading)).
  It appears only once the work has run for 150 ms, so quick work does not
  flicker it
  (see [`cli.md`](cli.md) and [`build.md`](build.md#gui)), `assetc <version>` on the right in weak text.
- **Center**: the canvas, previewing the selected file in 2D (zoom, pan,
  tiling) or 3D (on a rotating shape), with a view bar on top; see
  [`canvas.md`](canvas.md).

## Menus

- **Bar items** (`File`, `Edit`, …): 14 px text, 12 px horizontal padding,
  each item exactly as tall as the bar's content (the bar has no vertical
  margin), items edge to edge with no gaps. The hover / open highlight fills
  the item's whole rectangle with square corners, so it covers the full item,
  not just its text.
- **Dropdown items**: full-width rows, 28 px tall, 14 px text, at least
  180 px wide. Hovering highlights the whole row (`BG_HEADER`, square
  corners) and the whole row is clickable. A 24 px gutter on the left shows a
  `check` icon for toggles that are on (empty when off).
- `File → Quit` exits the application. Its shortcut, `Ctrl+Q` (`Cmd+Q` on
  macOS), works anywhere, even while a text field has focus, and is shown
  right-aligned in the item in `TEXT_WEAK`.
- `Edit` contains no items yet (button shown disabled).
- `View → Left Sidebar` / `View → Right Sidebar` toggle each sidebar (check
  shown when visible); the menu stays open after toggling.
- `Help` shows the application name and version as a non-interactive weak
  row, then **Licenses**, which opens the [Open source](#open-source) dialog.

### Open source

A modal dialog listing the open source projects assetc is built with, in
the dialog look of [file operations](file-operations.md) (`BG_PANEL`, 1 px
`STROKE` border, 8 px corners, 24 px margin):

- Title `Open source` (20 px, `TEXT`), then `assetc is built with these open
  source projects:` (14 px, `TEXT_WEAK`).
- A four-column grid, one row per project, 14 px: the name (`TEXT`), what
  assetc uses it for, the license (both `TEXT_WEAK`) and the project's site
  as a link in `ACCENT` (opened in the default browser).
- A **Close** button at the bottom right (the dialog's Cancel look); Escape
  or a click outside the dialog closes it too.

| Project              | Used for               | License                     | Site |
|----------------------|------------------------|-----------------------------|------|
| Lucide               | icons                  | ISC                         | lucide.dev |
| Ubuntu               | typeface               | Ubuntu Font Licence 1.0     | design.ubuntu.com/font |
| Hack                 | monospace typeface     | MIT                         | sourcefoundry.org/hack |
| Noto Emoji           | emoji                  | SIL Open Font License 1.1   | fonts.google.com/noto |
| Bevy                 | application framework  | MIT or Apache-2.0           | bevy.org |
| wgpu                 | graphics               | MIT or Apache-2.0           | wgpu.rs |
| winit                | windowing              | Apache-2.0                  | github.com/rust-windowing/winit |
| egui                 | user interface         | MIT or Apache-2.0           | github.com/emilk/egui |
| bevy_egui            | egui integration       | MIT                         | github.com/vladbat00/bevy_egui |
| resvg                | drawing icons          | Apache-2.0 or MIT           | github.com/linebender/resvg |
| gltf                 | reading models         | MIT or Apache-2.0           | github.com/gltf-rs/gltf |
| meshoptimizer        | meshlets, levels of detail | MIT                     | meshoptimizer.org |
| meshopt-pure         | meshoptimizer in Rust  | MIT                         | github.com/qbart/meshopt-pure |
| image                | reading images         | MIT or Apache-2.0           | github.com/image-rs/image |
| ttf-parser           | reading fonts          | MIT or Apache-2.0           | github.com/harfbuzz/ttf-parser |
| ab_glyph             | font atlases           | Apache-2.0                  | github.com/alexheretic/ab-glyph |
| serde and toml       | settings files         | MIT or Apache-2.0           | serde.rs |
| globset              | path patterns          | Unlicense or MIT            | github.com/BurntSushi/ripgrep |
| clap                 | command line           | MIT or Apache-2.0           | github.com/clap-rs/clap |
| base64               | embedded glTF data     | MIT or Apache-2.0           | github.com/marshallpierce/rust-base64 |
| webbrowser           | opening links          | MIT or Apache-2.0           | github.com/amodm/webbrowser-rs |
| BLAKE3               | hashing in Bevy, pure Rust | CC0-1.0 or Apache-2.0   | github.com/BLAKE3-team/BLAKE3 |

Links open `https://` + the site shown. The list names direct dependencies
and the fonts and icons shipped in the binary; keep it in step with
`Cargo.toml`. `blake3` is a direct dependency only to turn on its `pure`
feature, so Bevy's copy is built without C or assembly.

## File browser

Each tab shows a tree of the directory configured in `assetc.toml`
(`input` / `output`, see [`docs/spec/assetc.toml.md`](../spec/assetc.toml.md)).

- Each row: chevron (`chevron-right` / `chevron-down`, folders only), icon
  by type (see [Tree icons](#tree-icons)), name. Rows are 22 px tall, indented
  14 px per level, and highlight with `BG_HEADER` on hover. Clicking a folder
  row opens / closes it **and** selects it; clicking a file row selects it
  (see [`inspector.md`](inspector.md#selection)).
  Right-clicking an Input row opens the [file operations](file-operations.md)
  menu.
- **Sidecars**: a file's [`.amod`](../spec/amod.md) (`Fox.glb.amod` for
  `Fox.glb`) is listed right below that file, one level deeper, instead of in
  name order. It does not fold: the sidecar row is always shown, and the file
  has no chevron. Sidecar rows are
  dimmed: icon and name in `TEXT_WEAK`. A sidecar whose file is missing stays
  in name order, still dimmed. Sidecar rows select, rename and delete like any
  file.
- Folders are collapsible (collapsed by default) and listed before files;
  each group is sorted by name, case-insensitively.
- Entries whose name starts with `.` are hidden.
- Symlinks are listed as plain entries and never followed.
- Directories are scanned when the app opens and when the rescan button is clicked; there
  is no live file watching.
- Placeholders (weak text) instead of a tree:
  - no project: "No project open"
  - directory missing: "<path> does not exist"
  - directory empty: "Empty"
  - read error: the error message

### Tree icons

The icon follows the entry's type, by extension (any case):

| Entry                                         | Icon                      |
|-----------------------------------------------|---------------------------|
| environment map folder (`*.env`)              | `sun-medium`              |
| other folder                                  | `folder` / `folder-open`  |
| image: `png`, `jpg`, `jpeg`, `gif`, `bmp`, `tga`, `webp`, `tif`, `tiff`, `exr`, `hdr`, `dds`, `ktx2` | `image` |
| font: `ttf`, `otf`, `woff`, `woff2`, `afont`  | `type`                    |
| color LUT: `cube`                             | `aperture`                |
| noise / pattern: `noise`                      | `image-play`              |
| gradient: `gradient`                          | `swatch-book`             |
| palette: `palette`                            | `palette`                 |
| mesh: `gltf`, `glb`, `obj`, `amesh`          | `torus`                   |
| animation: `aanim`                            | `film`                    |
| per-file settings: `amod`                     | `settings-2` (dimmed)     |
| anything else                                 | `file`                    |

## Tabs

Tabs are attached to the pane they switch, not drawn as separate buttons:

- The tab strip has `BG_BAR` background and a 1 px `STROKE` line along the
  edge facing the pane.
- The active tab is filled with the pane's color (`BG_PANEL`), covers that
  line so it flows into the pane, and has a 2 px `ACCENT` indicator on its
  outer edge.
- Inactive tabs are unfilled with `TEXT_WEAK` text/icons; hover brightens
  them to `TEXT`.
- Left sidebar tabs (horizontal): 32 px tall, 14 px text, 12 px horizontal
  padding, indicator on the top edge.
- Inspector tabs (vertical): 36 × 36 px, 18 px icon, indicator on the left
  edge.
- Disabled tabs (not applicable right now): `TEXT_WEAK` at 50% opacity, no
  hover, not clickable. The Inspector hides tabs that do not apply instead,
  see [`inspector.md`](inspector.md#tabs).

## Icons

All icons are [Lucide](https://lucide.dev) SVGs, copied unmodified from
`lucide-static` into `assets/icons/` together with Lucide's ISC `LICENSE`, and
embedded in the binary. Their `currentColor` stroke is rendered white and
tinted per use. In use: `info` (inspector tabs), `refresh-cw`
(rescan), `hammer` (build), `files` (Inspector Assets tab), `film` (Inspector Animations tab), `images` (Inspector Mipmaps tab),
`bring-to-front` (Inspector Levels of detail tab), `map-pin` (Inspector
Locators tab, compiled mesh locator toggle), `move-3d`, `rotate-3d` (locator
gizmo modes), `crosshair` (placing locators on the surface),
`flip-horizontal-2` (mirroring locators), `plus`, `trash-2`, `refresh-cw`
(adding, deleting and reverting locators), `shapes` (Inspector Shapes tab),
`paint-bucket` (Inspector Regions tab), `paintbrush` (painting regions),
`list` (Inspector Properties tab), `check` (menu
toggles), `grid-3x3` (canvas tiling), `circle-small` (canvas channel icons), `box`, `globe`, `cylinder`, `torus`
(canvas 3D shapes), `palette` (LUT reference chart, Inspector Gradients and Colors tabs, `.palette` files), `eye`, `eye-off` (LUT cube before / after, locator visibility),
`sliders-vertical` (Inspector Parameters tab), `wrench`,
`grip-horizontal`, `trash-2`, `plus` (Inspector Modifiers tab),
`grip-vertical` (LUT compare divider), `box-checker`, `box-shaded`, `box`, `squares-unite`, `image`, `boxes` (compiled mesh display modes), `loader`
(status bar), `folder`, `folder-open`, `file`, `sun-medium`, `type`,
`aperture`, `image-play`, `swatch-book`, `image`, `torus`, `settings-2`, `chevron-right`, `chevron-down` (file tree,
see [Tree icons](#tree-icons)), `image` (texture assets). Two icons are assetc's own, drawn
on Lucide's `box` (same outline, stroke and license): `box-checker` (each
visible face split 2 × 2 with two opposite cells filled solid in the
outline's color, a checkerboard) and `box-shaded` (the top face filled at 90%, the left at 55%,
the right at 30%, so it reads as lit). `eye-outline` is Lucide's `eye`
without its pupil, for the canvas [Color vision](canvas.md#color-vision)
buttons, which draw their colored ban sign in its place. The logo is not a Lucide icon, see
below.

## Logo

Source: `assets/icons/pipe_logo.png` (an orange pipe splitting into three,
64 × 64 px, transparent background), embedded in the binary. The previous
logo, `assets/icons/logo.png` (anvil), stays in the repository but is not
used. At startup it is cropped to the bounding box of its visible pixels
(alpha > 8 of 255, so near-invisible noise in the margins is ignored), then resized (Lanczos3) to 16 pt tall at the window's scale factor,
keeping its aspect ratio, and shown untinted at the left of the menu bar.

## Property editor

Inspector tabs that edit values (Parameters, Modifiers) are laid out like a
property sheet, inside 8 px margins:

- **Sections**: cards with 6 px rounded corners and `BG_HEADER` fill, 8 px
  apart. The header row (28 px) has a `chevron-down` / `chevron-right` icon
  and the title (14 px, `TEXT`); clicking the chevron or title collapses /
  expands the card (open by default unless a feature says otherwise,
  remembered for the session). Cards may
  be non-collapsible: no chevron, always open. Optional header buttons sit
  after the chevron (before the title) and on the right. The body is inset
  10 px.
- **Rows**: 26 px tall, 6 px apart. The label (13 px, `TEXT_WEAK`) is
  right-aligned in a column 40% of the card width (at least 80 px); the
  control fills the rest.
- **Controls**: text and number fields, dropdowns and buttons fill the
  control column, with 4 px rounded corners and `BG_BAR` fill.
  **Sliders** take the control column minus 64 px, followed by a 56 px value
  box (a number field) 8 px to their right.

## Dialogs

Modal dialogs (e.g. the delete confirmation) dim the window behind them and
share one look:

- **Frame**: `BG_PANEL` fill, 1 px `STROKE` border, 8 px rounded corners,
  window shadow, 24 px inner margin; content 360 px wide.
- **Title**: 16 px, `TEXT`. **Message**: 14 px, `TEXT_WEAK`, 8 px below the
  title, wrapping.
- **Buttons**: right-aligned in one row 24 px below the message, 8 px apart,
  each at least 96 × 34 px with 6 px rounded corners and 14 px text. The
  confirming button is last (rightmost) with `ACCENT` fill and `BG_BAR`
  text; **Cancel** has `BG_HEADER` fill, a 1 px `STROKE` border and `TEXT`.
  Hovering brightens either fill.
- Escape or clicking outside the dialog cancels.

## Look

Colors live as constants in `src/theme.rs`.

Dark theme, flat panels separated by 1 px lines, single accent color for
selection and active elements.

| Token          | Color     | Used for                              |
|----------------|-----------|---------------------------------------|
| `BG_CENTER`    | `#2B2B2D` | window clear color                    |
| `BG_PANEL`     | `#1F1F22` | sidebars                              |
| `BG_BAR`       | `#18181A` | menu bar, status bar, tab strip       |
| `BG_HEADER`    | `#252528` | section headers                       |
| `STROKE`       | `#2E2E32` | panel separators                      |
| `TEXT`         | `#D4D4D8` | regular text                          |
| `TEXT_WEAK`    | `#7A7A82` | placeholders, secondary text          |
| `ACCENT`       | `#F0883E` | selection, active tab                 |
| `CHECKER_LIGHT`| `#3A3A3E` | transparency checkerboard (light)     |
| `CHECKER_DARK` | `#2E2E32` | transparency checkerboard (dark)      |
| `CHANNEL_R`    | `#E5484D` | red channel icon                      |
| `CHANNEL_G`    | `#46A758` | green channel icon                    |
| `CHANNEL_B`    | `#3E8EED` | blue channel icon                     |

## Keybindings

`Ctrl+Q` / `Cmd+Q` quits; see [`KEYS.md`](../../KEYS.md).
