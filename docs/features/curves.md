# Curves

A **curve** maps time `x` (0..1) to a value `y`: tuning such as falloffs,
easing and spawn rates. It is the `curve` type of
[`.aschema`](../aschema.md#32-curves); schemas edit curve defaults with the
[curve editor](#curve-editor) (see [`schemas.md`](schemas.md#defaults)).

## Model

A curve is a list of **points** (keyframes) sorted by `x`, each four
numbers, as Unity's AnimationCurve keyframes:

| Part  | Meaning |
|-------|---------|
| `x`   | Time, in 0..1 |
| `y`   | Value at that time; not limited, so below 0 or above 1 is overshoot |
| `in`  | Slope (dy/dx) arriving at the point from the left |
| `out` | Slope (dy/dx) leaving the point to the right |

- A curve has **at least two points**, sorted by `x`, each `x` in 0..1.
- Two points may share an `x`: the zero-length segment between them is
  skipped, a step.
- The default curve is linear: `(0, 0)` and `(1, 1)`, all slopes 1.

## Evaluating

- Before the first point: the first point's `y`; after the last: the last
  point's `y` (flat holds).
- Between neighbours `p` and `q`: [matma](https://crates.io/crates/matma)'s
  cubic Hermite with `d = q.x − p.x`, `t = (x − p.x) / d`, from `p.y` with
  tangent `d · p.out` to `q.y` with tangent `d · q.in`. Slopes are scaled by
  the segment's width, so a slope means the same whatever the spacing.
- `slope(x)` is the derivative of the same polynomial divided by `d`
  (0 outside the points). A point added at `x` with `y = eval(x)` and
  `in = out = slope(x)` keeps the curve's shape.

Zero slopes at both ends give smoothstep; slopes equal to the line between
two points give that line. Whether a point's slopes are joined has no effect
on evaluating.

## Curve editor

A popover (opened by double-clicking a curve's preview), 320 px wide, in the
[dialog look](gui.md#dialogs); clicking outside it or `Escape` closes it.
Top to bottom:

### Presets

A row of 8 tiles (36 × 24 px, the curve's miniature in `ACCENT` on
`BG_PANEL`, a 1 px `STROKE` frame; hovering shows the name). Clicking one
replaces the curve:

| Preset      | Points `[x, y, in, out]` |
|-------------|--------------------------|
| Linear      | `[0, 0, 1, 1]`, `[1, 1, 1, 1]` |
| Ease in     | `[0, 0, 0, 0]`, `[1, 1, 2, 2]` (y = x²) |
| Ease out    | `[0, 0, 2, 2]`, `[1, 1, 0, 0]` |
| Ease in-out | `[0, 0, 0, 0]`, `[1, 1, 0, 0]` (smoothstep) |
| Constant    | `[0, 1, 0, 0]`, `[1, 1, 0, 0]` |
| Overshoot   | `[0, 0, 4.702, 4.702]`, `[1, 1, 0, 0]` (back-out: peaks near 1.1) |
| Bounce      | `[0, 0, 0, 0]`, `[0.364, 1, 5.5, -2.75]`, `[0.727, 1, 2.75, -1.375]`, `[0.909, 1, 1.375, -0.688]`, `[1, 1, 0.688, 0.688]` (bounce-out, split slopes) |
| Step        | `[0, 0, 0, 0]`, `[0.5, 0, 0, 0]`, `[0.5, 1, 0, 0]`, `[1, 1, 0, 0]` |

### Plot

- **Fixed scale**: 200 px per unit on both axes, so a slope's angle on
  screen is its angle in the data and a curve looks the same every time.
- **The frame**: `x` from −0.1 to 1.1, `y` from −0.25 to 1.25: the unit
  square (filled `BG_HEADER`, 1 px `TEXT_WEAK` at 40% border) with space
  around it, on `BG_PANEL`. When the curve (its points or its line) goes
  further than 0.1 inside that space, the frame grows to 0.1 past it, at the
  same scale, up to `y` −1..2. The frame keeps its size while something is
  dragged.
- **Grid**: a line every 0.1 (`STROKE`), whole numbers brighter
  (`TEXT_WEAK` at 50%); labels every 0.5 (11 px `TEXT_WEAK`): `y` in a
  28 px column on the left, `x` in a 16 px row below.
- **The curve**: a 2 px `ACCENT` line, sampled every pixel across the whole
  frame, flat holds included.
- **Points**: 4 px radius dots in `TEXT`; the chosen point 6 px, `ACCENT`.
- **Tangent handles** of the chosen point: 40 px strokes from the point
  along `(1, slope)`, the out handle to the right, the in handle to the
  left, each ending in a 3 px dot.
  - **Joined** slopes: one straight stroke through the point, `TEXT_WEAK`.
  - **Split** slopes: two strokes in `CHANNEL_B`, so the state shows before
    anything is touched.
  - **While a handle is dragged**: a 9 px `ACCENT` ring around the point,
    and the dragged handle (both, when joined) `ACCENT` and 56 px long.
- **Playhead**: right of the plot, after an 8 px gap, a 12 px **lane**
  (`BG_HEADER`) where a 5 px radius `ACCENT` disc moves up and down at
  `eval(x)` while `x` sweeps 0..1; a 1 px `CHANNEL_R` line across the plot
  marks `x`. One pass takes 2 s and loops; it plays when the editor opens.
  While playing the editor redraws every 20 ms; paused, it requests no
  frames.

### Readout and fields

Under the plot: the chosen point's status, 12 px `TEXT_WEAK`:
`point 2 of 3`, then `· split` when its slopes are split; then its **X**,
**Y**, **In**, **Out** number fields (three decimals) to read and type
exact values (`X` stays between the neighbours; on a joined point a typed
slope sets both). With no point chosen: `<n> points · press one to edit
it`.

### Buttons

A row of icon buttons: **Play / Pause** (`play` / `pause`), **Add point**
(`plus`), **Delete point** (`trash-2`; disabled on the first and last
points and when two remain), **Split / Join** (`unlink-2` on a joined
point, `link-2` on a split one). The play state is kept for the session,
not in the data.

### Editing

Every edit keeps the [model](#model)'s rules.

| Edit | Behaviour |
|------|-----------|
| **Choose** | Press a point; `[` / `]` the previous / next point (wrapping), `Home` / `End` the first / last; pressing empty plot chooses none |
| **Move** | Drag a point. `x` is clamped between the neighbours' and to 0..1 (no re-sorting); `y` is free |
| **Exact** | The X, Y, In, Out fields |
| **Add** | **Add point** or `A`: halfway between the chosen point and the next (from the last point: halfway back to the previous), with `y = eval(mid)` and `in = out = slope(mid)`, so the shape is unchanged; joined and chosen. **Double-click** empty plot: the same at that `x` |
| **Delete** | **Delete point**, `Delete` / `X`, or double-click a point; refused on the first and last points and when two remain |
| **Rotate** | Drag a tangent handle: its slope follows the pointer's angle from the point, clamped to −89°..89°, so slopes reach about ±57 |
| **Split / Join** | Joined (equal to three decimals) slopes turn together, keeping the curve smooth through the point. **Split** (`S`, the button, or `Shift` while starting a handle drag) marks the point split without writing anything; the next handle drag then makes the slopes differ, and the data says split from then on. The mark is forgotten when another point is chosen. **Join** (`S` or the button on a split point) copies `out` into `in` |
| **Preset** | Click a tile |
| **Play** | `Space` or the button pauses and resumes the playhead where it is |

Keys act while the pointer is over the editor and no text field is
focused. Values are written with three decimals. An empty curve value is
filled with the linear default when the editor opens; any other value that
is not a curve shows its error and **Reset** (to linear).

## Not yet

- Undo, scrubbing the playhead, ping-pong playback and an adjustable pass
  duration.
