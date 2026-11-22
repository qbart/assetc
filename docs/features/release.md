# Release

Pushing a version tag builds the `assetc` program (the CLI and GUI, package
`assetc-core`) in release mode for three platforms and publishes the
binaries as a GitHub release. It is a GitHub Actions workflow,
`.github/workflows/release.yml`.

The [SDK](sdk.md) and [`assetc-colors`](colors.md) are not part of it: the
crates are only published by hand, with `make publish` (see
[Publishing](colors.md#publishing)).

## Trigger

- A pushed tag `v<version>` (`v0.2.0`, `v0.3.0-beta.1`), where `<version>` is
  the `version` of `assetc-core` in the root `Cargo.toml`.
- A tag that does not match that version fails the release before anything
  is built: `tag v0.3.0 does not match assetc-core 0.2.0 in Cargo.toml`.
  Set the [version](#version) everywhere, commit, push, then tag that
  commit:

  ```
  $ git tag v0.2.0
  $ git push origin v0.2.0
  ```

- Other pushes and tags not starting with `v` run nothing.

## Version

The version is written in several places, and a release is tagged only when
all of them hold the new one, in one pushed commit:

| Place | What |
|-------|------|
| `Cargo.toml` | `version` of `assetc-core`: the one the tag is checked against, and what the program reports (`assetc --version`, the status bar) |
| `sdk/Cargo.toml` | `version` of the `assetc` crate (the SDK), kept the same |
| `colors/Cargo.toml` | `version` of the [`assetc-colors`](colors.md) crate, kept the same |
| `demo/Cargo.toml` | `version` of `assetc-demo`, kept the same |
| `Cargo.toml` `[dependencies]` | the `version` of the `assetc-colors` path dependency |
| `Cargo.lock` | the four packages' entries; `cargo update --workspace` writes them. The release builds with `--locked`, so a stale lock file fails it |
| `docs/features/gui.md` | the `assetc <version>` shown in the status bar's example |

To check, before tagging (`0.2.0` being the old version; hits in
`Cargo.lock` for other crates and the example tags in this file do not
count):

```
$ cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | "\(.name) \(.version)"'
$ git grep -n '0\.2\.0' -- . ':!docs/features/release.md'
$ cargo build --locked
```

## Builds

One job per platform, in parallel:

| Platform | Runner | Target | Archive |
|----------|--------|--------|---------|
| Linux x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `assetc-<version>-linux-x64.tar.gz` |
| macOS Apple silicon | `macos-14` | `aarch64-apple-darwin` | `assetc-<version>-macos-arm64.tar.gz` |
| Windows x64 | `windows-2022` | `x86_64-pc-windows-msvc` | `assetc-<version>-windows-x64.zip` |

Each job:

1. Checks out the tag and installs stable Rust for the target.
2. On Linux, installs the system libraries Bevy builds against (ALSA, udev,
   Wayland, xkbcommon, X11 headers). Ubuntu 22.04 is used so the binary runs
   on distributions with glibc 2.35 or newer.
3. Runs the workspace tests (`cargo test --workspace --locked`), so a release
   is only made from a tree that passes on every platform.
4. Builds `cargo build --release --locked -p assetc-core --target <target>`.
5. Packs the archive: a folder `assetc-<version>-<platform>/` holding the
   binary (`assetc`, or `assetc.exe` on Windows), `README.md`, `KEYS.md` and
   `NOTICE.md`. The binary is self-contained: icons, the logo and shaders are
   built into it.
6. Writes `<archive>.sha256` beside it (`<hex digest>  <archive name>`, as
   `sha256sum` prints it).

Builds are cached per platform (Cargo registry and `target/`), keyed by
`Cargo.lock`.

## Release

After all three builds succeed, one more job creates the GitHub release
named after the tag (`v0.2.0`), with release notes generated from the
commits since the previous tag, and uploads the three archives and their
`.sha256` files. A version with a pre-release part (`-beta.1`) is marked as
a pre-release. If any build fails, no release is created.

The tag must point at a commit pushed to GitHub, and the workflow needs
`contents: write` permission for the release (granted in the workflow
file).

## Running the binaries

- **Linux**: `tar xzf`, then run `assetc`. Needs a Vulkan or OpenGL driver
  and X11 or Wayland for the GUI; the CLI commands need neither.
- **macOS**: the binary is not signed or notarized, so Gatekeeper blocks it
  when it came from a browser download: run
  `xattr -d com.apple.quarantine assetc` once, or allow it in System
  Settings ▸ Privacy & Security. Needs macOS 11 or newer.
- **Windows**: unzip and run `assetc.exe`; SmartScreen may warn about an
  unknown publisher, as the binary is not signed.
