# `.apaint` files

The texels of one [texture paint](../features/texture-paint.md) of a glTF
model, written by assetc next to the model.

## Name

`<model file name>.<paint name>.apaint`, in the model's folder: the full file
name of the model (with its extension) and the paint's name from the
model's [`.amod`](amod.md#paints) sidecar, e.g. `Fox.glb.dirt.apaint` for
paint `dirt` of `Fox.glb`. Paint names are ASCII letters, digits, `_` and
`-`, so the name is the same on every file system.

## Encoding

A [PNG](https://www.w3.org/TR/png-3/) image, nothing else (only the
extension differs, so the build does not take it for a texture source):

| Property     | Value |
|--------------|-------|
| Size         | width = height = the paint's `size` (256, 512, 1024, 2048 or 4096) |
| Color type   | `6`, RGBA |
| Bit depth    | `8` |
| Interlace    | none |
| Chunks       | `IHDR`, `IDAT`, `IEND` only: no gamma, color profile or text |

Rows are top to bottom, as the texture is (row 0 is `v = 0` of
`TEXCOORD_0`). Each texel is R, G, B, A, stored exactly:

- Color mode: an sRGB color with straight alpha.
- Channels mode: four linear values 0–255, one per channel.
- Unpainted: `0, 0, 0, 0`.

assetc reads any PNG the [`image`](https://docs.rs/image) crate reads
(other color types and bit depths are converted to RGBA, 8 bits), but
writes only the form above.

## Lifetime

- Written when a stroke ends, when the paint is cleared and when its size
  changes; not written before the first of these (a paint without a file is
  unpainted).
- Renamed with the paint, deleted with it, and moved, renamed and deleted
  with the model (see
  [`file-operations.md`](../features/file-operations.md#companions)).
