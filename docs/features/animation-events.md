# Animation events

An **event** is a named moment of one of a glTF model's animations: a
footstep, the frame a sword hits, where a sound starts. assetc does not know
what an event means; its name tells the game.

Events are an *enrichment* like [locators](locators.md): kept in the model's
[`.amod`](../spec/amod.md#events) sidecar, placed on a
[timeline](#timeline) under the 3D view, and compiled into the model's
[`.aanim`](../spec/aanim.md#events).

## Event

| Field     | Meaning |
|-----------|---------|
| Animation | The animation it belongs to, by name: the glTF name, or `Animation <n>` (from 1) without one, as the [Animations tab](models.md#animations-tab) lists it. |
| Name      | Non-empty free text (`footstep`, `hit`, `sound.whoosh`); names may repeat, also within one animation. |
| Time      | Seconds from the animation's start, 0 to its duration. |

## Timeline

While the Inspector's [Animations tab](models.md#animations-tab) is shown
for a model in the Input tree, a **timeline** panel takes the bottom 132 px
of the canvas (the 3D view above it shrinks; the panel is `BG_PANEL` with a
1 px `STROKE` line on top, 8 px side margins). It has, top to bottom:

1. **Header** (28 px): a **Play** / **Pause** icon button (`play` /
   `pause`), the time `0.42 / 1.25 s` (two decimals), the animation's name,
   and an **Add event** button (`plus` and text) that adds an event at the
   playhead. With **Rest pose** chosen, the header only has a weak
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
- **Press a marker**: chooses that event; **drag it** sideways to change its
  time (clamped to the animation). Within 6 px of the pointer, the nearest
  marker wins.
- **Double-click the track** where there is no marker: adds an event named
  `event` there and chooses it. **Double-click a marker**: deletes it.
- Choosing another animation chooses no event.
- Edits are saved like the locators'.

## Build

A model's events are written to its `.aanim` as the
[`EVENTS` and `EVNAMES`](../spec/aanim.md#events) chunks (none without
events), sorted by animation (file order), then time:

- An event of an animation the model does not have fails the build:
  `<file>.amod: event "<name>": no animation "<animation>"`.
- An event past its animation's end fails it: `<file>.amod: event
  "<name>": time <t> is past the end of "<animation>" (<duration> s)`.
- Events on a model without an `.aanim` (no skin) fail it: `<file>.amod:
  events need a skinned model`.

## Inspector Info

- A model in the Input tree: an **Events** row after Paths: their count, or
  `none`.
- A compiled `.aanim`: an **Events** row after Keys: their count, or `none`.

## Not yet

- Properties on events, and event kinds or tags.
- Events from the glTF (no standard), copying events between animations,
  snapping to keyframes.
- Scrubbing faster than real time, playback speed, frame stepping.
