# Animation events

An **event** is a named moment of one of a glTF model's animations: a
footstep, the frame a sword hits, where a sound starts. assetc does not know
what an event means; its name tells the game.

Events are an *enrichment* like [locators](locators.md): kept in the model's
[`.amod`](../spec/amod.md#events) sidecar, listed in each animation's card
of the [Animations tab](#event-list), placed on a [timeline](#timeline)
under the 3D view, and compiled into the model's
[`.aanim.bin`](../spec/aanim.bin.md#events).

## Event list

The unfolded card of the [Animations tab](models.md#animations-tab) lists
its animation's events:

- **Add event** (`plus` and text): an event named `event` at the playhead,
  chosen.
- One row per event, by time (ties in sidecar order), 26 px: its time
  (`0.334 s`, monospace, `TEXT_WEAK`, 64 px wide), name and, at its right
  end, **Delete** (`trash-2` icon button). Hovering fills the row with
  `BG_HEADER`; the chosen event's row is filled with `ACCENT` at 25%
  opacity. Clicking a row chooses the event (and clicking the chosen one
  chooses none); the timeline shows the same choice.
- Under the chosen event's row, its fields: **Name** (text), **Time** (drag
  field, seconds, three decimals, 0 to the duration) and **Properties** (the
  [property editor](properties.md#properties-tab)).
- Without events, a weak `No events`.
- Edits are saved like the locators'.

## Event

| Field     | Meaning |
|-----------|---------|
| Animation | The animation it belongs to, by name: the glTF name, or `Animation <n>` (from 1) without one, as the [Animations tab](models.md#animations-tab) lists it. |
| Name      | Non-empty free text (`footstep`, `hit`, `sound.whoosh`); names may repeat, also within one animation. |
| Time      | Seconds from the animation's start, 0 to its duration. |
| Properties | Key / value pairs carried to the game (a sound's name and volume, a hit's damage), as the [model's](properties.md#property). |

## Timeline

While the Inspector's [Animations tab](models.md#animations-tab) is shown
for a model in the Input tree, a **timeline** panel takes the bottom 132 px
of the canvas (the 3D view above it shrinks; the panel is `BG_PANEL` with a
1 px `STROKE` line on top, 8 px side margins). It has, top to bottom:

1. **Header** (28 px): a **Play** / **Pause** icon button (`play` /
   `pause`), the time `0.420 / 1.250 s` (three decimals, so `Shift` steps
   show), the animation's name,
   and an **Add event** button (`plus` and text) that adds an event at the
   playhead. In the rest pose (every card folded), the header only has a weak
   `Choose an animation to edit its events`, and nothing below.
2. **Ruler** (20 px): the animation from 0 (left) to its duration (right),
   with a tick every step and a label (`0.5 s`, 11 px, `TEXT_WEAK`) on
   every one; the step is the smallest of 0.05, 0.1, 0.25, 0.5, 1, 2, 5, 10
   and 30 s whose labels are at least 60 px apart.
3. **Track** (32 px, `BG_HEADER`, 4 px corners): one marker per event of the
   animation: a 10 px diamond in `TEXT` at its time (the chosen event's in
   `ACCENT`), its name in 11 px `TEXT_WEAK` to its right. Markers flash in
   `ACCENT` for 0.25 s when the playhead passes them while playing.
4. **Event row** (28 px), for the chosen event: **Name** (text field),
   **Time** (drag field, seconds, three decimals, 0 to the duration) and
   **Delete** (`trash-2`). Without a chosen event a weak
   `Double-click the track to add an event`.

The **playhead** is a 1 px `ACCENT` line across the ruler and track with a
small triangle on the ruler, at the animation's current time.

Interactions:

- **Play** / **Pause** (or `Space` with the pointer over the panel and no
  text field focused) pauses and resumes the animation where it is. Choosing
  an animation plays it from its start.
- **Press or drag on the ruler**: moves the playhead there (the pose
  follows) and pauses.
- **`,`** / **`.`** (no text field focused, pointer anywhere): pauses and
  steps the playhead back / forward to the previous / next multiple of
  0.01 s (from 0.304 s: back to 0.30 s, forward to 0.31 s; from 0.30 s: to
  0.29 s and 0.31 s), within 0 to the duration. With **`Shift`**, by
  multiples of 0.001 s instead (from 0.3045 s: 0.304 s and 0.305 s). Held,
  they repeat.
- **`E`** (no text field focused, pointer anywhere) toggles an event at the
  playhead: the animation's event nearest the playhead within 0.0005 s is
  deleted; without one, an event named `event` is added at the playhead
  (rounded to 0.001 s) and chosen. The 3D view's `E` (gizmo Rotate) does
  not apply while an animation is chosen, as transforms are locked then.
- **Press a marker**: chooses that event; **drag it** sideways to change its
  time (clamped to the animation). Within 6 px of the pointer, the nearest
  marker wins.
- **Double-click the track** where there is no marker: adds an event named
  `event` there and chooses it. **Double-click a marker**: deletes it.
- Choosing another animation chooses no event.
- Edits are saved like the locators'.

## Build

A model's events are written to its `.aanim.bin` as the
[`EVENTS` and `EVNAMES`](../spec/aanim.bin.md#events) chunks (none without
events), sorted by animation (file order), then time:

- An event of an animation the model does not have fails the build:
  `<file>.amod: event "<name>": no animation "<animation>"`.
- An event past its animation's end fails it: `<file>.amod: event
  "<name>": time <t> is past the end of "<animation>" (<duration> s)`.
- Events on a model without an `.aanim.bin` (no skin) fail it: `<file>.amod:
  events need a skinned model`.

## Inspector Info

- A model in the Input tree: an **Events** row after Paths: their count, or
  `none`.
- A compiled `.aanim.bin`: an **Events** row after Keys: their count, or `none`.

## Not yet

- Event kinds or tags.
- Events from the glTF (no standard), copying events between animations,
  snapping to keyframes.
- Scrubbing faster than real time, playback speed, frame stepping.
