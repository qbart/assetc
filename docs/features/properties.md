# Properties

**Properties** are key / value pairs on a glTF model that the game reads with
it: a mass, a display name, whether it can be picked up, a number of lives.
assetc does not know what they mean.

They are an *enrichment* like [locators](locators.md): kept in the model's
[`.amod`](../spec/amod.md#properties) sidecar, edited in the Inspector's
[Properties tab](#properties-tab), and compiled into its
[`.amesh`](../spec/amesh.md#properties). [Shapes](shapes.md) carry
properties of their own, edited the same way.

## Property

| Part  | Meaning |
|-------|---------|
| Key   | Non-empty, unique within the model (or the shape). The game finds it by the FNV-1a-64 hash of the key. Case is kept. |
| Type  | **Bool** (`true` / `false`), **Int** (64-bit signed), **Float** (64-bit) or **Text** (UTF-8). |
| Value | Of that type. |

Properties are kept sorted by key (in the sidecar, the Inspector and the
compiled file).

## Properties tab

The Inspector's **Properties** tab (`list`) is shown for a model in the
Input tree. In the [property editor](gui.md#property-editor) look, inside
8 px margins:

- One row per property, 26 px tall: the **key** (a text field, 40% of the
  width), the **type** (a dropdown: `Bool`, `Int`, `Float`, `Text`, 64 px),
  the **value** (a checkbox, an integer drag field, a float drag field with
  three decimals at 0.01 per point, or a text field, filling the rest) and a
  **Delete** icon button (`trash-2`).
- The key is applied when its field loses focus (or Enter is pressed): an
  empty key or one used by another property gets the old key back.
- Changing the type converts the value where it can: Bool ↔ Int as 0 / 1,
  Int ↔ Float, a number to Text as written, Text to a number when it parses
  (otherwise 0, or `false`).
- Below the rows, **Add property** (`plus` and text) adds `property`, or
  `property_<n>` (the smallest free `n` from 2), of type Float with value 0.
- With no properties, a weak `No properties` line precedes the button.
- Edits are [saved](locators.md#saving) like locators', without reloading
  the model in the 3D view.

The same editor, with the same rules, edits a [shape's](shapes.md)
properties inside its card.

## Build

The model's properties are written to its `.amesh` as the
[`PROPS`](../spec/amesh.md#properties) chunk (none without properties), their
keys and text values in [`STRINGS`](../spec/amesh.md#strings).

## Inspector Info

- A model in the Input tree: a **Properties** row after Locators: their
  count, or `none`.
- A compiled mesh: a **Properties** row after Locators.
- The [Chunks view](models.md#chunks-view) shows `PROPS` (Key, Type, Value).

## Not yet

- Properties from the glTF (`extras` of the scene) and from presets.
