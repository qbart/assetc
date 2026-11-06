# Attached structs

Any [schema](schemas.md) struct can be **attached** to an asset, so the
asset carries typed game data next to its own: a `QuestStep` on every node
of a quest graph, a `Pickup` on a 3D model, a `Surface` on a material, a
`Biome` on each color of a palette. The values are filled in assetc with the
same editors as [instances](instances.md), obey the same schema rules, and
are compiled into the asset's own binary, resolved, in the
[struct chunks](../spec/astruct.md).

## Scopes

An attached struct belongs to a **scope**: the asset itself, or each item of
a kind the asset is made of. Every **owner** of a scope (the asset, or each
node, edge, gradient or color) has a value of every struct attached to the
scope.

| Asset | Kept in | Scopes, with their keys |
|-------|---------|-------------------------|
| Graph (`*.agraph`) | the file | **Graph** `structs`, **Node** `node_structs`, **Edge** `edge_structs` |
| Tree (`*.atree`) | the file | **Tree** `structs`, **Node** `node_structs` |
| 3D model (`*.gltf`, `*.glb`) | its [`.amod`](../spec/amod.md#structs) sidecar | **Model** `structs` |
| Material (`*.amat`, `*.acmat`) | the file | **Material** `structs` |
| Atlas (`*.atlas` folder) | its [`atlas.toml`](../spec/atlas.toml.md#structs) | **Atlas** `structs` |
| Gradient file (`*.agradient`) | the file | **File** `structs`, **Gradient** `gradient_structs` |
| Palette (`*.apalette`) | the file | **File** `structs`, **Color** `color_structs` |

- An **attachment** names a struct: `schema`, the schema's path relative to
  the folder of the file that holds the attachment (forward slashes, as a
  data file's [`$schema`](instances.md#schema)), and `type`, the struct's
  name. Attachments of a scope keep their order.
- Structs from several schemas may be attached to one scope. A struct is
  attached at most once per scope (its values are kept by its name); the
  same struct may be attached to several scopes.
- **Values** are kept per owner and struct, sparse as an instance's
  ([§4.2](../aschema.md#42-sparse-storage)): what the owner sets, the rest
  inherited from the fields' defaults, else their types' implicit defaults.
  They are written [normalised](../spec/adata.md#written-form) as an
  instance's fields are: a value equal to the one it would inherit is left
  out, a struct with nothing set is left out, and `values` is left out when
  empty.
- Detaching a struct removes its values from every owner of the scope.

How each file keeps attachments and values is in its spec:
[`.agraph`](../spec/agraph.md#structs), [`.atree`](../spec/atree.md#structs),
[`.amod`](../spec/amod.md#structs), [`.amat`](../spec/amat.md#structs),
[`atlas.toml`](../spec/atlas.toml.md#structs),
[`.agradient`](../spec/agradient.md#structs),
[`.apalette`](../spec/apalette.md#structs). JSON files use JSON values; TOML
files use the same values as TOML (see [Values in TOML](#values-in-toml)).

### Values in TOML

The a/1 data values map onto TOML one to one: `true` / `false`, integers,
floats (written with a decimal point), strings (colors, enums, refs and
asset paths too), arrays (lists, curves, gradients) and tables (struct
values). TOML has no `null`, so a `null` (an optional field set to no value)
is the inline table `{ "$null" = true }`; `$`-keys are reserved in a/1, so it
cannot be mistaken for a struct value.

## Structs tab

The Inspector's **Structs** tab (`boxes`) is shown for every asset of the
table above in the Input tree, in the
[property editor](gui.md#property-editor) look, inside 8 px margins.

**Attached**: one section per scope of the asset, in the table's order:

- A header (`Graph structs`, `Node structs`, …; 14 px, `TEXT_WEAK`).
- A hint under it (12 px, `TEXT_WEAK`, wrapped) saying whose values the
  scope holds and where they are filled:

  | Scope | Hint |
  |-------|------|
  | Graph | One value for the whole graph. Fill it with no node or edge selected. |
  | Tree | One value for the whole tree. Fill it with no node selected. |
  | Node (graph, tree) | Every node gets its own value of these. Select a node to fill it. |
  | Edge | Every edge gets its own value of these. Select an edge to fill it. |
  | Model, Material, Atlas | One value for the model (material, atlas). |
  | File (gradients, palettes) | One value for the whole file. Choose File under Values of. |
  | Gradient | Every gradient gets its own value of these. Choose one under Values of. |
  | Color | Every color gets its own value of these. Choose one under Values of. |
- One row per attachment, 26 px: the struct's name (monospace, `TEXT`), the
  schema path (`TEXT_WEAK`, right-aligned), the [problem
  icon](instances.md#problems) of the attachment's own problems and
  **Detach** (`x`).
- **Attach struct** (`plus` and text): a menu with one submenu per schema of
  the input directory (by its path relative to the asset's file, sorted by
  input-relative path), listing its structs in the schema's order; a struct
  already attached to the scope is disabled. A schema that cannot be read
  shows its error as a disabled item. Choosing one appends it to the scope.
- With no attachments, a weak `None` under the header.

**Values**: a header `Values of <owner>`, then a card per struct attached
to the owner's scope:

- **Owner**:
  - Graphs and trees: what the canvas (or the Graph / Tree tab) has chosen: `Values of graph` (`tree`)
    with nothing chosen, `Values of node <id>`, `Values of edge <from> ->
    <to>`. Choose another in the canvas or the [Graph tab](graphs.md#graph-tab).
  - Gradient files and palettes: the header is `Values of` and a dropdown
    after it: `File`, then each gradient (color) as `<n>  <name>` (from 1;
    `<n>` alone without a name). `File` to start with; the choice is kept per file for the
    session.
  - Models, materials and atlases: the asset itself, `Values of model`
    (`material`, `atlas`).
  - An owner whose scope has no attachments: a weak `No structs attached to
    the <asset>` (`No structs attached to the graph`) or `No structs attached
    to <item>s` (`No structs attached to nodes`) instead of cards.
- **A card** (`BG_HEADER`, 6 px corners, 8 px padding, 8 px apart): the
  struct's name (14 px, its `doc` on hover) and the schema path
  (`TEXT_WEAK`), then its fields as an instance's
  [fields](instances.md#canvas) are edited: sorted by `order`, in groups,
  with conditions, optional checkboxes, readonly fields, Reset and the
  problem icons. `ref<S>` dropdowns list the
  [project](instances.md#references) of the struct's schema; `asset<K>`
  dropdowns the input's files of that kind.
- A struct whose schema cannot be read, or that is not a struct of it: the
  card shows the error (`CHANNEL_R`) and no fields.

Edits are written as the asset's other edits are: graphs and trees as the
[Graph tab](graphs.md#graph-tab) writes them, models' sidecars as the
[Locators tab](locators.md#saving) writes them, materials as the
[Material tab](materials.md#saving), atlases' `atlas.toml` as the
[Atlas tab](atlases.md#inspector), gradients and palettes as their
Parameters tab; once nothing is being dragged or typed.

## Problems

Checked whenever the asset or its schemas change, against the projects of
the schemas for refs:

| Check | Level |
|-------|-------|
| An attachment whose schema cannot be read: `<Type>: schema <path>: <error>` | error |
| An attachment whose `type` is not a struct of its schema | error |
| The same struct attached twice to a scope | error |
| Values of a struct that is not attached to the owner's scope (`values for <Type>, which is not attached`); kept | warning |
| Every check of an instance's values ([Problems](instances.md#problems)): values that do not encode as their type are errors; out-of-range numbers, unset required refs, missing asset paths, refs to no instance and unknown keys are warnings | as there |

Problems show as the problem icon on the attachment's row (its own) and on
the fields of the cards (theirs), and as a line under the Values header
for problems of other owners (`node dash: Step: <message>`), so nothing
wrong is out of sight. Problems never block saving.

## Build

The asset's binary carries every owner's value of every attached struct,
**resolved** (every field set, as in [`.adata.bin`](../spec/adata.bin.md)),
in the [struct chunks](../spec/astruct.md):

| Asset | Binary |
|-------|--------|
| Graph | [`.agraph.bin`](../spec/agraph.bin.md) |
| Tree | [`.atree.bin`](../spec/atree.bin.md) (nodes in its breadth-first order) |
| Model | [`.amesh.bin`](../spec/amesh.bin.md) |
| Material | [`.amat.bin`](../spec/amat.bin.md#struct-data), after its record |
| Atlas | [`.aatlas.bin`](../spec/aatlas.bin.md) |
| Gradient file | [`.agradient.bin`](../spec/agradient.bin.md) |
| Palette | [`.apalette.bin`](../spec/apalette.bin.md) |

- An asset without attachments has no struct chunks.
- Each attachment records its struct's name and the asset ID of its
  schema's `.aschema.bin`, so a game can read the types.
- A `ref<S>` is compiled with its target's place, as in
  [`.adata.bin`](../spec/adata.bin.md#target-24-bytes): the target is an
  instance of the project of the struct's schema.
- Errors fail the asset's build: `<owner> <Type>: <message>` of the first
  (`node dash Step: …`), `(and <n> more)` when there are more; warnings do
  not.
- An [incremental build](cli.md#assetc-watch) compiles an asset with
  attachments again whenever any `.aschema` or `.adata` file changes, as
  its values depend on its schemas' defaults and the projects' instances.

## SDK

The [SDK](sdk.md#attached-structs) reads the struct chunks of every format
above into the format's `structs` field.

## Not yet

- Attaching structs to an atlas's frames, a model's locators, shapes or
  paths, and textures.
- Copying values between owners.
