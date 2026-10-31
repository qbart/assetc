# CLI and Project Config

## Commands

```
assetc            Open the GUI for the project containing the current directory
assetc init       Create assetc.toml in the current directory
assetc build      Compile input into output, see build.md
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

- Window title is `assetc — <project root directory name>`, or `assetc`
  when no project was found.
- Left sidebar browses the configured `input` and `output` directories (see
  [`gui.md`](gui.md#file-browser)).
- Status bar shows `Ready` when a project was loaded, or
  `No assetc.toml found (run assetc init)` otherwise.
