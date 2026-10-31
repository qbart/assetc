# GUI Shell

The application window is an egui shell drawn over Bevy. It provides the
frame that later features plug into; it has no asset functionality of its own.

## Layout

```
+--------------------------------------------------------------+
| ◆  File  Edit  View  Help                         (menu bar) |
+-------------+------------------------------+-----------------+
| Input|Output|                              | Assets       +  |
|             |                              |                 |
|             |     (empty, no canvas)       |-----------------|
|             |                              | ℹ | Inspector   |
|             |                              | ⚙ |             |
+-------------+------------------------------+-----------------+
| Ready                                     assetc 0.1.0       |
+--------------------------------------------------------------+
```

- **Menu bar** (top, fixed height): logo (see [Logo](#logo)) followed by the
  `File`, `Edit`, `View`, `Help` menus.
  - Each menu item's click/hover area spans the full bar height with 10 px
    horizontal padding, and items sit edge to edge with no gaps, so the whole
    item is clickable, not just its text.
  - `File → Quit` exits the application.
  - `Edit` contains no items yet (button shown disabled).
  - `View → Left Sidebar` / `View → Right Sidebar` toggle each sidebar.
  - `Help` shows the application name and version.
- **Left sidebar** (resizable, default width 260 px, min 200 px): file
  browser with two tabs in its header row, **Input** and **Output**, plus
  build (`hammer` icon, see [`build.md`](build.md#gui)) and rescan
  (`refresh-cw` icon) buttons on the right. See [File browser](#file-browser).
- **Right sidebar** (resizable, default width 320 px, min 220 px), split
  vertically into two sections:
  - **Assets** (top, ~40% height): header row with title and a disabled
    `plus` icon button; body shows a weak "No assets" placeholder.
  - **Inspector** (bottom): a narrow vertical tab strip on the left with icon
    tabs (`Info`, `Settings`, see [Tabs](#tabs)), and the active tab's content on the right.
    Both tabs show a weak "Nothing selected" placeholder for now.
- **Status bar** (bottom, fixed height): status message on the left
  (see [`cli.md`](cli.md) and [`build.md`](build.md#gui)), `assetc <version>` on the right in weak text.
- **Center**: no canvas. The window clear color fills the remaining space.

## File browser

Each tab shows a tree of the directory configured in `assetc.toml`
(`input` / `output`, see [`docs/spec/assetc.toml.md`](../spec/assetc.toml.md)).

- Each row: chevron (`chevron-right` / `chevron-down`, folders only), icon
  (`folder` / `folder-open` / `file`), name. Rows are 22 px tall, indented
  14 px per level, and highlight with `BG_HEADER` on hover; clicking a folder
  row toggles it.
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

## Icons

All icons are [Lucide](https://lucide.dev) SVGs, copied unmodified from
`lucide-static` into `assets/icons/` together with Lucide's ISC `LICENSE`, and
embedded in the binary. Their `currentColor` stroke is rendered white and
tinted per use. In use: `info`, `settings` (inspector tabs), `refresh-cw`
(rescan), `hammer` (build), `plus` (Assets header), `folder`, `folder-open`, `file`,
`chevron-right`, `chevron-down` (file tree). The logo is not a
Lucide icon, see below.

## Logo

Source: `assets/icons/logo.png` (anvil, transparent background), embedded in
the binary. At startup it is cropped to the bounding box of its non-transparent
pixels, then resized (Lanczos3) to 16 pt tall at the window's scale factor,
keeping its aspect ratio, and shown untinted at the left of the menu bar.

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

## Keybindings

None. (Any added later must be documented in `KEYS.md`.)
