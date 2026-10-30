# GUI Shell

The application window is an egui shell drawn over Bevy. It provides the
frame that later features plug into; it has no asset functionality of its own.

## Layout

```
+--------------------------------------------------------------+
| ◆  File  Edit  View  Help                         (menu bar) |
+-------------------------------------------+------------------+
|                                           | Assets        +  |
|                                           |                  |
|            (empty, no canvas)             |------------------|
|                                           | ℹ | Inspector    |
|                                           | ⚙ |              |
+-------------------------------------------+------------------+
| Ready                                     assetc 0.1.0       |
+--------------------------------------------------------------+
```

- **Menu bar** (top, fixed height): accent-colored logo mark followed by the
  `File`, `Edit`, `View`, `Help` menus.
  - `File → Quit` exits the application.
  - `Edit` contains no items yet (button shown disabled).
  - `View → Sidebar` toggles the sidebar.
  - `Help` shows the application name and version.
- **Sidebar** (right, resizable, default width 320 px, min 220 px), split
  vertically into two sections:
  - **Assets** (top, ~40% height): header row with title and a disabled `+`
    button; body shows a weak "No assets" placeholder.
  - **Inspector** (bottom): a narrow vertical tab strip on the left with icon
    tabs (`Info`, `Settings`), and the active tab's content on the right.
    Both tabs show a weak "Nothing selected" placeholder for now.
- **Status bar** (bottom, fixed height): status message on the left
  (`Ready`), `assetc <version>` on the right in weak text.
- **Center**: no canvas. The window clear color fills the remaining space.

## Look

Dark theme, flat panels separated by 1 px lines, single accent color for
selection and active elements.

| Token          | Color     | Used for                              |
|----------------|-----------|---------------------------------------|
| `BG_CENTER`    | `#2B2B2D` | window clear color                    |
| `BG_PANEL`     | `#1F1F22` | sidebar                               |
| `BG_BAR`       | `#18181A` | menu bar, status bar, tab strip       |
| `BG_HEADER`    | `#252528` | section headers                       |
| `STROKE`       | `#2E2E32` | panel separators                      |
| `TEXT`         | `#D4D4D8` | regular text                          |
| `TEXT_WEAK`    | `#7A7A82` | placeholders, secondary text          |
| `ACCENT`       | `#6E56CF` | logo, selection, active tab           |

## Keybindings

None. (Any added later must be documented in `KEYS.md`.)
