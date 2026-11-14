# `.amipmap` files

The mip chain of one texture source: how many levels it is written with and
which of them come from files instead of being generated. See
[`docs/features/mipmaps.md`](../features/mipmaps.md). assetc reads it when
it builds and inspects the texture, and writes it from the Inspector's
Mipmaps tab.

## Name

`<texture file name>.amipmap`, in the texture's folder: the full file name
of the texture (with its extension) plus `.amipmap`, e.g. `rock.png.amipmap`
for `rock.png`. The texture is an image (`.png`, `.jpg`, `.jpeg`, `.tga`) or
an `.anoise` file. An `.amipmap` file is not a source of its own (the build
counts it as skipped).

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8. Every key is optional; an
empty file means "a full chain, levels by name or generated". Unknown tables
or keys are errors (the texture's build then fails, with
`<texture file name>.amipmap: <problem>`).

| Key       | Type    | Default          | Meaning |
|-----------|---------|------------------|---------|
| `levels`  | integer | the full chain   | number of levels written, level 0 included: `1` or more. More than the texture's full chain (`floor(log2(max(width, height))) + 1`) fails the build. |
| `[files]` | table   | empty            | level files, see below |

### `[files]`

One key per level that comes from a chosen file:

| Key   | Type   | Meaning |
|-------|--------|---------|
| `<n>` | string | the image of level `n`: a decimal integer without sign or leading zeros, `1` or more, and below `levels` when that is set. The value is a non-empty path relative to the `.amipmap`'s folder, `/`-separated, naming a texture source in the input directory. |

Level 0 is the texture itself and cannot be set.

A level without a key comes from the file named `<name>.mip<n>.<ext>` in the
same folder (`<name>` the texture's file name without its extension, `<ext>`
a texture source extension), or is generated from the level above it.

## Example

```toml
# rock.png.amipmap: six levels; 1 and 3 from chosen files, 2 from
# rock.mip2.png if it exists, the rest generated.
levels = 6

[files]
1 = "rock_half.png"
3 = "../shared/gray32.png"
```

## Written form

assetc writes `levels` first (when set), an empty line, then `[files]` (when
it has keys) with its keys in ascending order. Comments and layout of a
hand-written file are not kept when the Inspector saves it; when files are
[moved](../features/file-operations.md#paths), only the path strings change.
