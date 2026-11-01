# `.noise` procedural textures

A `.noise` file is a texture source made of parameters instead of pixels. The
build generates the image from them (see
[`docs/features/noise.md`](../features/noise.md)); the file itself stays as
parameters. assetc writes these files when one is created or its parameters
are edited in the Inspector.

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8, extension `.noise` (any case).
Unknown keys are rejected. Every key is optional; missing keys take the
defaults below.

| Key         | Type    | Range                                   | Default    |
|-------------|---------|-----------------------------------------|------------|
| `type`      | string  | `"white"`, `"value"`, `"perlin"`, `"worley"` | `"perlin"` |
| `size`      | integer | `1..=4096`, width = height in pixels    | `512`      |
| `seed`      | integer | `0..=4294967295`                        | `0`        |
| `frequency` | integer | `1..=256`, lattice cells across the image (first octave) | `4` |
| `octaves`   | integer | `1..=8`                                 | `4`        |
| `gain`      | float   | `0..=1`, amplitude factor per octave    | `0.5`      |
| `invert`    | bool    |                                         | `false`    |

`frequency`, `octaves` and `gain` are ignored by `white`.

### Written form

When assetc writes a `.noise` file, it writes every key, in the order of the
table, one per line, `key = value`, using TOML's standard formatting (strings
in double quotes, floats with a decimal point), followed by a newline, e.g.:

```toml
type = "perlin"
size = 512
seed = 0
frequency = 4
octaves = 4
gain = 0.5
invert = false
```

## Generated image

`size × size` pixels, one gray value `v` in `0..1` per pixel stored as
`R = G = B = round(v × 255)`, `A = 255` (a grayscale image). The image tiles
seamlessly: every lattice is periodic with a whole number of cells across the
image.

### Hash

All randomness comes from

```
hash(x, y, s) = splitmix64((s as u64) << 32 | (x & 0xFFFF) << 16 | (y & 0xFFFF))
splitmix64(z):  z += 0x9E3779B97F4A7C15
                z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
                z = (z ^ (z >> 27)) * 0x94D049BB133111EB
                return z ^ (z >> 31)
unit(h) = (h >> 40) as f64 / 2^24                       (in [0, 1))
```

with wrapping 64-bit arithmetic; `x` and `y` are always below 65536 (pixel
coordinates and lattice indices are under 4096 × 2⁷). A second independent value from the same
lattice point uses `hash(x, y, s ^ 0x5bd1e995)`.

### Sample position

Pixel `(px, py)` samples at `u = (px + 0.5) / size`, `v = (py + 0.5) / size`
(both in `0..1`). An octave with period `p` cells looks at lattice coordinates
`(u × p, v × p)`; lattice indices are taken modulo `p`, so the octave tiles.

### Types

- **white**: `v = unit(hash(px, py, seed))`, one independent value per pixel.
- **value**: lattice values `unit(hash(i mod p, j mod p, seed'))` at integer
  points, interpolated with the quintic fade `f(t) = 6t⁵ − 15t⁴ + 10t³`.
- **perlin**: gradients at lattice points, angle
  `2π × unit(hash(i mod p, j mod p, seed'))`; the dot products with the
  offsets to the four corners are interpolated with the same fade; the result
  `n` (range ±√½) is mapped to `0.5 + n × √½`, clamped to `0..1`.
- **worley**: one feature point per cell at
  `cell + (unit(hash(i, j, seed')), unit(hash(i, j, seed' ^ 0x5bd1e995)))`
  (`i, j` wrapped modulo `p`); the value is the Euclidean distance (in cells)
  to the nearest feature point among the 3 × 3 neighbouring cells, clamped to
  `0..1`.

### Octaves (value, perlin, worley)

Octave `o` (0-based) uses period `p = frequency × 2^o` and seed
`seed' = seed + o` (wrapping), and is weighted `gain^o`:

```
v = Σ gain^o × noise_o(u, v)  /  Σ gain^o        (o = 0 … octaves − 1)
```

(with `gain = 0`, only the first octave counts).

Then `invert` replaces `v` with `1 − v`.
