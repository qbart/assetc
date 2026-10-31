# Assets panel

Lists the assets of the last successful build, as recorded in
`<output>/assets.manifest` (see [`docs/spec/manifest.md`](../spec/manifest.md)).
Lives in the top part of the right sidebar (see [`gui.md`](gui.md)).

## Loading

- The manifest is read when the app opens and on every rescan (rescan button,
  or after a build finishes).
- Since a failed build does not write a manifest, the panel keeps showing the
  last successful build.

## List

- One row per manifest entry, sorted by asset path (case-insensitive), not by
  ID.
- Row: kind icon (`image` for textures) and the **asset path**: the entry's
  output path without its extension, e.g. `textures/Grass`. Hovering a row
  shows its asset ID (`0x` + 16 hex digits) as a tooltip.
- Rows look like file browser rows (22 px, `BG_HEADER` on hover; selected:
  `ACCENT` at 25% plus a 2 px `ACCENT` left bar).
- The list scrolls vertically when it does not fit.
- Placeholders (weak text) instead of a list:
  - no project: "No project open"
  - no manifest: "Not built yet"
  - manifest unreadable or invalid: the error message
  - manifest with no entries: "No assets"

## Selection

The panel shares the single selection with the file browser (see
[`inspector.md`](inspector.md#selection)):

- Clicking an asset row selects its compiled file in the output directory, so
  the Inspector shows that file and the Output tree highlights it (when its
  folder is open). The browser tab does not switch.
- An asset row is highlighted when the selection is:
  - its compiled file (Output), or
  - a source texture (Input `*.png`) whose asset ID equals the asset's ID.
- Other selections highlight no asset.

## Header

`Assets` title; the disabled `plus` button stays as before.

## Keybindings

None.
