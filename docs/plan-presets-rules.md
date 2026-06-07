# Plan: presets + richer per-file rules

Status: proposed. Extends the existing `assetc.yml` (`src/assetc/config.{hpp,cpp}`).

## Goals

1. **Presets** — named bundles of settings selected with `assetc --preset <name>`
   (e.g. `mobile`, `desktop`) so the same source tree builds to different targets
   (chiefly separate output dirs in v1).
2. **Texture rules beyond mesh** — per-pattern texture overrides; v1 ships exactly
   one knob, `compress: false`, to keep UI/data/sRGB textures pixel-exact.
3. **Layering** — a `default:` block defines the base; the selected preset
   *overlays* it. Rules live in both `default:` and each preset.

Non-goal: keep it composable but still small. No per-asset scripting, no
expression language — just declarative settings overlaid by specificity.

---

## Config schema

```yaml
# Top-level: project-wide, NOT part of the layering.
input: assets            # source tree — same for every preset
pack: false
preset: desktop          # preset used when --preset is omitted (optional)

# Base layer: applied to every build.
default:
  output: runtime        # base output dir (a preset may redirect it)
  mesh:
    merge: true
  texture:
    compress: true       # UASTC-encode (default). false = store raw, lossless.
  rules:
    - match: "ui/*"
      texture:
        compress: false  # UI atlases / data textures: keep pixels exact
    - match: "props/*.glb"
      mesh:
        merge: false

# Presets overlay `default` when selected. Same shape as `default`.
presets:
  desktop:
    output: runtime/desktop
  mobile:
    output: runtime/mobile
    rules:
      - match: "decals/*"
        texture:
          compress: false
```

`input` and `pack` are read only at the top level. `output`, `mesh`, `texture`,
and `rules` live in the layers (`default` + the selected preset). Every setting is
optional and falls back to a built-in default (UASTC compression, mesh merge on,
`output: runtime`). The current flat top-level `mesh:` / `rules:` keys are
**removed** and replaced by this layered schema — no back-compat shim (the config
module is new this session, so we rewrite it cleanly).

DX: unknown top-level/section keys are a **warning** (typo protection, e.g.
`textures:` vs `texture:`), and an unknown `--preset` is a hard error that lists
the available preset names.

### Texture scope for v1

Just one knob: **`compress`** (default `true`). `false` writes the image as a raw
`R8G8B8A8` KTX2 with no block compression — pixel-exact, for UI atlases, sprite
sheets, or sRGB/data textures you want untouched (optionally lossless Zstd to
shrink without changing values, like the LUT path already does). UASTC stays the
default encoder; **ETC1S, quality, max_size, channels, srgb are out of scope for
now** — the `texture:` map is the place they slot in later.

---

## Resolution / precedence (the core rule)

`input`/`pack` are plain top-level scalars (no layering). `output` and the
per-asset settings (`mesh`, `texture`, `rules`) are resolved by *overlaying*
layers from least to most specific; a layer only overrides fields it actually
sets.

`output` = `preset.output` if the preset sets it, else `default.output`, else
`runtime`.

Per-asset settings, for a given source file and active preset:

```
settings = built-in defaults
overlay( default.texture / default.mesh )            # base
overlay( preset.texture  / preset.mesh )             # preset overrides base
for r in default.rules:  if match(r, file): overlay(r.*)   # in file order
for r in preset.rules:   if match(r, file): overlay(r.*)   # preset rules last → win
```

So the specificity order, lowest → highest:

1. built-in defaults
2. `default:` globals
3. `preset:` globals
4. `default.rules` (cascading, in file order)
5. `preset.rules` (cascading, in file order)

Notes:
- **Cascade, not first-match.** All matching rules overlay; later rules win.
  This supersedes today's first-match-wins for `merge`, but because each field is
  optional the single-rule case behaves the same.
- Glob unchanged: `*` spans `/`, `?` is one char, matched against the source path
  relative to `input` (e.g. `models/court.glb`).

---

## Settings and how each maps to the encoder

Settings flow into the KTX encode in `src/deps/ktx.cpp` (kept a thin wrapper — it
receives params, no policy). Two call sites consume the texture setting:
- standalone images: `main.cpp` Color/Normal/Grayscale → the encoder;
- glTF images: `encode_material.cpp` (`useTexture` picks the mode) →
  `EncodeGltfImageToKtx2`.

| Setting | Type | Maps to |
| --- | --- | --- |
| `texture.compress` | bool (default true) | `true` → today's UASTC encode. `false` → write a raw `R8G8B8A8` KTX2 (sRGB/UNORM per the slot/suffix), no Basis/UASTC, optionally lossless Zstd. Pixel-exact. |
| `mesh.merge` | bool | existing `BuildFromGltf` node-transform baking. |

The raw path already has a precedent: `ktx::FromLut3DToKtx2` writes an
uncompressed `R8G8B8A8` KTX2 + Zstd. v1 generalizes that to 2D images.

Reserved for later (documented as the growth path, not built now):
`texture.{quality, max_size, channels, srgb, supercompress}`, `mesh.lods`,
ETC1S/ASTC encodings.

---

## C++ data model (`config.hpp`)

Replace the flat `Config` with overlay-friendly optionals:

```cpp
struct TextureSettings {
    std::optional<bool> compress;          // false = raw RGBA8 KTX2
    void overlay(const TextureSettings& o);// copy each set field from o
};
struct MeshSettings {
    std::optional<bool> merge;
    void overlay(const MeshSettings& o);
};
struct Rule  { std::string match; TextureSettings tex; MeshSettings mesh; };
struct Layer { std::optional<std::string> output; TextureSettings tex;
               MeshSettings mesh; std::vector<Rule> rules; };

struct Config {
    std::string input = "assets";              // top-level only
    bool        pack  = false;                 // top-level only
    std::string defaultPreset;                 // top-level `preset:`
    Layer defaultLayer;                        // `default:`
    std::map<std::string, Layer> presets;      // `presets:`

    // output for the active preset (preset.output ?: default.output ?: "runtime").
    std::string outputFor(const std::string& preset) const;

    // Fully-populated per-file settings under an active preset.
    struct Resolved { bool compress; bool merge; };
    Resolved resolve(const std::string& relPath, const std::string& preset) const;

    bool hasPreset(const std::string& name) const;
    std::vector<std::string> presetNames() const;
};

int LoadConfig(const std::string& startDir, Config&, std::string& foundPath);
bool GlobMatch(const std::string& pattern, const std::string& str) noexcept; // unchanged
```

`resolve()` overlays default→preset globals then default→preset matching rules
(cascade), substituting built-in defaults (`compress=true`, `merge=true`) for any
field still unset. The struct grows field-by-field as future knobs land.

---

## Encoder changes (`deps/ktx` stays thin)

One new thin entry point for the uncompressed path; policy stays in the caller.

```cpp
// Raw R8G8B8A8 KTX2 (sRGB or UNORM per `mode`'s colour space), no Basis/UASTC.
// Mirrors FromLut3DToKtx2's raw+Zstd approach, for 2D images with mips.
KTX_error_code ktx::FromImageToRawKtx2(const stb::Image&, const std::string& dst,
                                       UASTCMode mode, unsigned threadCount);
```

- `FromImageToUASTC` is unchanged (the `compress: true` default path).
- `FromImageToRawKtx2`: build the texture with `VK_FORMAT_R8G8B8A8_SRGB/UNORM`
  (sRGB for the Color mode, UNORM otherwise — same rule as the UASTC path), fill
  mip levels, skip the Basis compress, apply `ktxTexture2_DeflateZstd` (lossless),
  write. No swizzle (raw means raw).
- Callers pick the function by the resolved `compress` flag. The `.ktx2`
  inspector already prints format + supercompression, so raw textures show up as
  `R8G8B8A8_*` with `Zstandard` automatically.

---

## Wiring

- **CLI**: add `--preset <name>`; effective preset = `--preset` else
  `config.preset:` else `""` (defaults only). Add `--list-presets` to print names.
  Unknown preset → error listing available ones. `-o` still overrides the resolved
  output.
- **Output**: `outputDir = config.outputFor(preset)` (unless `-o` given). Drives
  the build dir, cache, manifest, and `--pack` target.
- **Standalone images** (`main.cpp` Color/Normal/Grayscale): pick
  `FromImageToUASTC` vs `FromImageToRawKtx2` by `resolve(rel, preset).compress`.
- **glTF images** (`encode_material.cpp` / `EncodeGltfImageToKtx2`): thread the
  resolved `compress` in so a rule (e.g. UI textures referenced by a glTF) can
  force raw too.
- **Cache**: fold the resolved settings (`compress` + `merge`) and the active
  preset name into the per-asset hash seed in `main.cpp`, so switching `--preset`
  or editing a rule rebuilds the affected assets. (Today only `merge` is folded
  in.)
- **`assetc init`**: extend the template (`WriteDefaultConfig`) to show
  top-level `input`/`pack`/`preset`, a `default:` block, and a `presets:` example
  — only `input` and `default.output` active, the rest commented.

---

## Tests (extend `tests/test_config.cpp`, add encoder coverage)

- `resolve()` precedence: built-in < default-global < preset-global <
  default-rule < preset-rule; unset fields fall through each layer.
- Cascade: two matching rules, later wins per-field; non-overlapping fields merge.
- `outputFor()`: preset output overrides default; falls back to `runtime`.
- Preset selection: unknown preset errors; `preset:` default vs `--preset`.
- `compress: false` on `ui/*` resolves to raw for matching files, UASTC otherwise.
- Unknown key (e.g. `textures:`) warns but still loads.
- Encoder: a tiny image through `FromImageToRawKtx2` yields a valid KTX2 whose
  header is `R8G8B8A8_*` (not UASTC) with Zstandard supercompression set.

---

## Delivery steps (commit after each)

1. Rewrite the config data model: optionals + `Layer`/`Rule` + `resolve()` +
   `outputFor()` + unknown-key warnings. Replaces the current flat `Config`. Tests
   for resolution/precedence/output. (No output change yet — defaults reproduce
   today's behavior.)
2. `--preset` / `--list-presets` CLI + `preset:` default + `outputFor()` wired to
   the build dir + cache-seed folding (compress/merge/preset).
3. Encoder: `FromImageToRawKtx2`; route the standalone-image path by `compress`.
   Tests (raw header / Zstd).
4. Route the glTF texture path through resolved `compress`.
5. `assetc init` template + README (`Configuration` section: presets, layering,
   `compress`, `--preset`, per-preset output).

---

## Decisions (chosen for best UX/DX)

- **`input` top-level, `output` layered.** One source tree regardless of preset;
  per-preset output dirs so `--preset mobile`/`desktop` builds coexist without
  clobbering. `pack`/`preset` are top-level too.
- **Cascade-overlay rules** (not first-match). All matching rules apply in order,
  later wins per-field — composes cleanly with the default→preset layering and
  lets a rule tweak one field without restating the rest.
- **Minimal texture scope: only `compress`.** UASTC stays the default; ETC1S /
  quality / max_size / channels / srgb are explicitly deferred. The single knob
  that matters now is "don't touch these pixels."
- **No back-compat, no preset inheritance.** Clean rewrite; `extends:` skipped.
- **Typo protection on.** Unknown keys warn; unknown `--preset` errors with the
  list of names.

### Still open (one decision)

- **Per-preset output default**: when a preset doesn't set `output`, use
  `default.output` as-is (explicit, predictable — **leaning this way**) *or*
  auto-suffix the preset name (`runtime` → `runtime/mobile`, never clobbers).
