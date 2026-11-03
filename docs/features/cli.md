# CLI and Project Config

## Commands

```
assetc            Open the GUI for the project containing the current directory
assetc init       Create assetc.toml in the current directory
assetc build      Compile input into output (the default target), see build.md
assetc build --target <name>
                  Compile input into the target's output with its settings
assetc watch      Build, then rebuild what changes until stopped (Ctrl+C)
assetc watch --target <name>
                  The same for a target
assetc --help     Print usage
assetc --version  Print version
```

### `assetc init`

- Writes `assetc.toml` (see [`docs/spec/assetc.toml.md`](../spec/assetc.toml.md))
  into the current working directory.
- Fails with a non-zero exit code if `assetc.toml` already exists in that
  directory; the existing file is never overwritten.
- Does not look at parent directories: running `init` inside an existing
  project creates a nested project, like `git init`.
- Prints the path of the created file on success.

### `assetc build`

- Builds the **default target**: the top-level settings of `assetc.toml`,
  into its `output` (see [`build.md`](build.md#cli)).
- `--target <name>` builds the [target](../spec/assetc.toml.md#targets)
  `[targets.<name>]` instead: the top-level settings with the target's
  replacing them, into the target's output (`<output>-<name>` by default).
  The last line names that output (`Built … into /path/build-mobile`).
- An unknown target fails before building: `error: no target "tablet"
  (targets: mobile, preview)`, or `(no targets)`.
- The GUI's Build button and `Ctrl+B` build the default target.

### `assetc watch`

Keeps the output up to date while you edit the input: a build, then an
**incremental** build after every change, until stopped with `Ctrl+C`.

- Loads the project and target as [`assetc build`](#assetc-build) does
  (`--target <name>` the same way); a missing project or an unknown target
  fails before watching.
- Prints `Watching <input> into <output> (Ctrl+C to stop)`, then builds
  everything once and prints the result as `assetc build` does (built lines,
  `removed …`, `error: …`, the [summary](build.md#summary) with
  ` into <output>`).
- **Watching**: every 0.5 s it lists the input directory as the build reads it
  (entries starting with `.` skipped, symlinks not followed, the output
  directory left out when it lies inside the input), with each file's size
  and modification time, plus `assetc.toml`. A difference from the last
  listing is a **change**: a file edited, added or deleted (also by renaming
  or moving, or with its folder). An added file is built as any other new
  source; a deleted one's outputs are removed as
  [stale](build.md#removing-stale-outputs), and assets that read it (a data
  file's schema, a material's textures) are compiled again. It waits until two listings 0.5 s apart agree (so a
  save that writes several files builds once), then prints an empty line and
  builds.
- **Incremental build**: the build is planned as usual (stale outputs
  removed, the manifest written), but an asset is compiled again only when
  - it was not built successfully in this session, or one of its outputs is
    missing;
  - its job changed: the settings the plan gives it (color space, mipmaps,
    mesh options, its `.amod` sidecar with inherited locators, a material's
    slots);
  - or a file it reads changed (size or modification time):

    | Asset | Files it reads |
    |-------|----------------|
    | texture, LUT, HDR map, font | its source |
    | ORM texture | its channel sources |
    | `.glb` model | the file |
    | `.gltf` model | every file in its folder and below (buffers and images it may refer to) |
    | atlas | every file in the `.atlas` folder |
    | schema, graph, tree | its source |
    | data file | its source and every `*.aschema` in the input |
    | material | its `.mat.toml` (when it has one) |

  Every other asset is **up to date**: not written, counted in the summary as
  `<U> up to date` after the skipped count (`Built 1 texture (0 skipped, 16
  up to date)`), left out when 0. Outputs written along with a model, font
  or atlas follow it.
- A change to `assetc.toml` reloads it (and the target); then everything is
  built again, as on start. An invalid config prints `error: …` and waits for
  the next change, keeping the last good one.
- A failed build (`Build failed: …`, or an error that stops it, such as a
  missing input directory) is printed and watching goes on; failed assets are
  compiled again on the next change, whatever changed.
- What it knows about past builds lives only while it runs: the first build
  of every `assetc watch` compiles everything. `assetc build` always compiles
  everything.

### `assetc` (no subcommand)

Discovers and loads the project config, then opens the GUI.

## Config discovery

Every time assetc opens, it looks for the project config the way git finds
`.git`:

1. Start at the current working directory.
2. If `assetc.toml` exists there, that file is the config and its directory
   is the **project root**.
3. Otherwise move to the parent directory and repeat.
4. Stop at the filesystem root.

If no `assetc.toml` is found, assetc opens with the default (empty) config
and no project root. If a file is found but cannot be read or parsed, assetc
prints the error and exits with a non-zero code.

## GUI integration

- Window title is `assetc`, with or without a project.
- Left sidebar browses the configured `input` and `output` directories (see
  [`gui.md`](gui.md#file-browser)).
- Status bar shows `Ready` when a project was loaded (`Processing…` while
  background work runs, see [`gui.md`](gui.md#layout)), or
  `No assetc.toml found (run assetc init)` otherwise.
