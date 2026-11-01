# `.noise` procedural textures

A `.noise` file is a texture source made of parameters instead of pixels: a
**generator** with its own parameters, and an ordered list of **modifiers**.
The build generates the image from them (see
[`docs/features/noise.md`](../features/noise.md)); the file itself stays as
parameters. assetc writes these files when one is created or edited in the
Inspector.

## Encoding

[TOML 1.1](https://toml.io/en/v1.1.0), UTF-8, extension `.noise` (any case).

| Key           | Type            | Meaning                                         | Default          |
|---------------|-----------------|-------------------------------------------------|------------------|
| `type`        | string          | generator, see [Generators](#generators)        | `"noise/perlin"` |
| `size`        | integer         | `1..=4096`, width = height in pixels            | `512`            |
| *parameters*  | per generator   | only the keys of the chosen generator           | per generator    |
| `[[modifiers]]` | array of tables | applied in order, see [Modifiers](#modifiers) | none             |

Every key is optional. A key that the chosen generator (or a modifier's type)
does not have is an error, so a file only ever holds the parameters of its own
generator. Integers must be TOML integers; floats accept integers too.

### Written form

When assetc writes a `.noise` file it writes `type`, `size`, then every
parameter of the generator in the order of its table below, one per line
(`key = value`), then each modifier as a `[[modifiers]]` table with `type`,
`enabled` and its parameters in table order, separated by blank lines:

```toml
type = "noise/fbm"
size = 512
seed = 0
basis = "perlin"
frequency_x = 4
frequency_y = 4
translate_x = 0.0
translate_y = 0.0
octaves = 5
lacunarity = 2
gain = 0.5
warp = 0.0

[[modifiers]]
type = "levels"
enabled = true
black = 0.1
white = 0.9
gamma = 1.0
```

## Generators

Parameter types: *int* (TOML integer), *float*, *choice* (a string from the
list), *gray* (float `0..=1`), *color* (array of 3 floats `0..=1`). All ranges
are inclusive; a value outside its range is an error.

Most generators share these parameters (listed per generator below, in this
order where present):

| Key             | Type, range                               | Default        | Meaning |
|-----------------|-------------------------------------------|----------------|---------|
| `seed`          | int 0–4294967295                          | 0              | random seed |
| `basis`         | choice `value` `perlin` `simplex`         | `perlin`       | [basic noise](#basic-noises) used |
| `interpolation` | choice `linear` `cubic` `quintic`         | `quintic`      | value noise curve (`noise/value` only) |
| `frequency_x`   | int 1–256                                 | per generator  | lattice cells across the image horizontally |
| `frequency_y`   | int 1–256                                 | per generator  | lattice cells down the image |
| `translate_x`   | float 0–1                                 | 0              | horizontal shift, in image widths |
| `translate_y`   | float 0–1                                 | 0              | vertical shift, in image heights |
| `octaves`       | int 1–8                                   | per generator  | number of fractal layers |
| `lacunarity`    | int 2–4                                   | 2              | frequency multiplier per octave |
| `gain`          | float 0–1                                 | 0.5            | weight multiplier per octave |
| `warp`          | float 0–4                                 | 0              | [domain warp](#domain) strength, in cells |

| `type`               | Parameters (key: type, range, default; shared keys by name)                     |
|----------------------|----------------------------------------------------------------------------------|
| `checkerboard`       | `cells_x`: int 1–256, 8 · `cells_y`: int 1–256, 8 · `translate_x` · `translate_y` · `color_a`: gray, 0 · `color_b`: gray, 1 |
| `noise/uniform`      | `seed`                                                                           |
| `noise/value`        | `seed` · `interpolation` · `frequency_x`, `frequency_y`: 8 · `translate_x` · `translate_y` · `octaves`: 1 · `lacunarity` · `gain` · `warp` |
| `noise/perlin`       | `seed` · `frequency_x`, `frequency_y`: 8 · `translate_x` · `translate_y` · `octaves`: 1 · `lacunarity` · `gain` · `warp` |
| `noise/simplex`      | as `noise/perlin`                                                                |
| `noise/fbm`          | `seed` · `basis` · `frequency_x`, `frequency_y`: 4 · `translate_x` · `translate_y` · `octaves`: 5 · `lacunarity` · `gain` · `warp` |
| `noise/voronoi`      | `seed` · `frequency_x`, `frequency_y`: 8 · `translate_x` · `translate_y` · `jitter`: float 0–1, 1 · `metric`: choice `euclidean` `manhattan` `chebyshev`, `euclidean` · `output`: choice `f1` `f2` `edge` `cell`, `f1` · `warp` |
| `noise/perlin-worley`| `seed` · `frequency_x`, `frequency_y`: 4 · `translate_x` · `translate_y` · `octaves`: 4 · `gain` · `worley_octaves`: int 1–4, 3 · `warp` |
| `noise/musgrave`     | `seed` · `basis` · `frequency_x`, `frequency_y`: 4 · `translate_x` · `translate_y` · `octaves`: 6 · `lacunarity` · `dimension`: float 0–2, 1 · `warp` |
| `noise/ridged`       | `seed` · `basis` · `frequency_x`, `frequency_y`: 4 · `translate_x` · `translate_y` · `octaves`: 6 · `lacunarity` · `dimension`: float 0–2, 1 · `offset`: float 0–2, 1 · `gain`: float 0–4, 2 · `warp` |
| `noise/curl`         | `seed` · `basis` · `frequency_x`, `frequency_y`: 4 · `translate_x` · `translate_y` · `octaves`: 2 · `lacunarity` · `gain` · `warp` |

(`noise/ridged`'s `gain` has its own range and default.)

## Generated image

`size × size` pixels, each an RGB value in `0..1`, stored as 8-bit
`round(c × 255)` with `A = 255`. Every generator except `noise/curl` produces
gray (`R = G = B`). Everything tiles seamlessly, since every lattice has a
whole number of cells across the image (`checkerboard` when `cells_x` and
`cells_y` are even).

Pixel `(px, py)` samples at `u = (px + 0.5) / size`, `v = (py + 0.5) / size`.
A lattice of periods `(p, q)` (cells across and down the image) uses
coordinates `(x, y) = (u × p, v × q)` and takes lattice indices `i` modulo
`p` and `j` modulo `q`.

### Hash

```
splitmix64(z): z += 0x9E3779B97F4A7C15
               z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
               z = (z ^ (z >> 27)) * 0x94D049BB133111EB
               return z ^ (z >> 31)
hash(x, y, s)        = splitmix64(splitmix64((s << 32) | x) ^ y)        (x, y, s: u32 as u64)
hash4(i, j, k, l, s) = splitmix64(splitmix64(splitmix64(splitmix64(s ^ i) ^ j) ^ k) ^ l)
                                                                          (i64 → u64 two's complement)
unit(h) = (h >> 40) / 2^24                                               (in [0, 1))
```

with wrapping 64-bit arithmetic. A second independent value at the same point
uses seed `s ^ 0x5bd1e995`.

### Basic noises (one lattice of periods `(p, q)`, seed `s`)

- **value(p, q, s)**: lattice values `unit(hash(i mod p, j mod q, s))`,
  interpolated with the curve `f`: quintic `6t⁵ − 15t⁴ + 10t³` (default),
  cubic `3t² − 2t³` or linear `t` (`interpolation`). Range 0–1.
- **perlin(p, q, s)**: gradient at each lattice point with angle
  `2π × unit(hash(i mod p, j mod q, s))`; the four corner dot products
  (gradient · offset to the sample) are interpolated with the quintic curve,
  giving `n` in ±√½; the result is `clamp(0.5 + n × √½, 0, 1)`.
- **simplex(p, q, s)**: 4D simplex noise (Gustavson's algorithm: skew
  `F4 = (√5 − 1)/4`, unskew `G4 = (5 − √5)/20`, radius² 0.6, scale 27) sampled
  on a torus so it tiles: with `r = p / 2π`, `t = q / 2π`, the point is
  `(r cos 2πu, r sin 2πu, t cos 2πv, t sin 2πv)`; each simplex corner's
  gradient is entry `hash4(i, j, k, l, s) mod 32` of the 32 vectors that are
  the permutations of `(0, ±1, ±1, ±1)` (zero in each of the four positions,
  ordered by zero position then signs `+++`, `++−`, `+−+`, `+−−`, `−++`, …).
  Result `clamp(0.5 + 0.5 n, 0, 1)`.
- **worley(p, q, s)**: one feature point per cell at
  `cell + (unit(hash(i, j, s)), unit(hash(i, j, s ^ 0x5bd1e995)))` (`i`
  modulo `p`, `j` modulo `q`); the Euclidean distance (in cells) to the
  nearest feature point among the 3 × 3 neighbouring cells, clamped to 0–1.

A *signed* basic noise is `2 × noise − 1`.

### Fractal sum

**fractal(noise)** with `octaves`, `lacunarity`, `gain`, `seed`: octave
`o = 0 … octaves−1` uses periods `(frequency_x × lacunarity^o, frequency_y × lacunarity^o)`,
seed `seed + o` (wrapping), weight `gain^o`; result
`Σ gain^o × noise / Σ gain^o`. With one octave it is the noise itself.

### Domain

Before a generator samples `(u, v)`, the sample position is moved:

1. **Translate** (generators with `translate_x`): `u += translate_x`,
   `v += translate_y`. Shifting by whole lattice periods wraps, so the result
   still tiles.
2. **Warp** (generators with `warp`, when `warp > 0`): with the generator's
   basis `B` (its own for `noise/value` / `noise/perlin` / `noise/simplex`,
   `basis` where present, else perlin, and for `noise/value` its
   `interpolation`), `a = B(frequency_x, frequency_y, seed ^ 0x68E31DA4)` and
   `b = B(frequency_x, frequency_y, seed ^ 0xB5297A4D)` at the translated
   `(u, v)`; then `u += warp × (2a − 1) / frequency_x`,
   `v += warp × (2b − 1) / frequency_y`.

### Generators

- **checkerboard**: `color_a` where `floor(u × cells_x) + floor(v × cells_y)`
  is even, else `color_b`.
- **noise/uniform**: `unit(hash(px, py, seed))` per pixel.
- **noise/value**, **noise/perlin**, **noise/simplex**: fractal of the basic
  noise of that name.
- **noise/fbm**: fractal of `basis`.
- **noise/voronoi**: feature point of cell `(i, j)` at
  `cell + 0.5 + jitter × (unit(hash(i, j, seed)) − 0.5, unit(hash(i, j, seed ^ 0x5bd1e995)) − 0.5)`
  (`i` modulo `frequency_x`, `j` modulo `frequency_y`), searched over the
  5 × 5 neighbouring cells with the chosen metric (`euclidean` √(dx²+dy²),
  `manhattan` |dx|+|dy|, `chebyshev` max(|dx|, |dy|)), distances in cells.
  `f1`: nearest distance; `f2`: second nearest; `edge`: second nearest minus
  nearest; `cell`: `unit(hash(i, j, seed ^ 0x2545F491))` of the nearest
  point's cell. Clamped to 0–1.
- **noise/perlin-worley**: `P` = fractal of perlin (`octaves`, lacunarity 2,
  `gain`); `W` = `1 −` (worley at periods
  `(frequency_x × 2^o, frequency_y × 2^o)`, seeds `seed + 100 + o`, weights
  `0.5^o`, `o = 0 … worley_octaves−1`, weighted average); result
  `clamp((P − W + 1) / (2 − W), 0, 1)` (Perlin remapped from `[W − 1, 1]`, as
  for volumetric clouds).
- **noise/musgrave** (multifractal): with signed `basis` noise `n_o` at
  periods `(frequency_x × lacunarity^o, frequency_y × lacunarity^o)`,
  `s = seed + o`, and `w_o = lacunarity^(−dimension × o)`:
  `∏ (1 + n_o w_o) / ∏ (1 + w_o)`.
- **noise/ridged** (ridged multifractal): `sig_0 = (offset − |n_0|)²`,
  `sum = sig_0`, `max = offset²`; for `o ≥ 1`:
  `weight = clamp(sig_(o−1) × gain, 0, 1)`, `sig_o = (offset − |n_o|)² × weight`,
  `sum += sig_o × w_o`, `max += offset² × w_o`; result `clamp(sum / max, 0, 1)`
  (`n_o`, `w_o` as for musgrave).
- **noise/curl**: potential `ψ` = signed fractal of `basis`; its gradient by
  central differences one pixel apart (wrapping at the edges), in pixels;
  vector `(∂ψ/∂y, −∂ψ/∂x)` divided by the largest vector length in the image
  (unchanged if all zero); stored as `R = 0.5 + 0.5 x`, `G = 0.5 + 0.5 y`,
  `B = 0`.

## Modifiers

Each `[[modifiers]]` table has `type`, `enabled` (bool, default `true`;
disabled modifiers are skipped but kept) and its own parameters. They run in
order on the RGB image, per channel unless stated, clamping to 0–1 after
each.

| `type`                | Parameters (key: type, range, default)                         | Operation |
|-----------------------|----------------------------------------------------------------|-----------|
| `invert`              | —                                                              | `1 − c` |
| `levels`              | `black`: float 0–1, 0 · `white`: float 0–1, 1 · `gamma`: float 0.1–10, 1 | `((c − black) / (white − black))^(1 / gamma)`; if `white ≤ black`, `c ≥ black ? 1 : 0` |
| `brightness-contrast` | `brightness`: float −1–1, 0 · `contrast`: float −1–1, 0          | `(c − 0.5) × (1 + contrast) + 0.5 + brightness` |
| `threshold`           | `level`: float 0–1, 0.5 · `softness`: float 0–0.5, 0             | `softness = 0`: `c ≥ level ? 1 : 0`; else smoothstep from `level − softness` to `level + softness` |
| `posterize`           | `steps`: int 2–32, 4                                             | `round(c × (steps − 1)) / (steps − 1)` |
| `blur`                | `radius`: int 1–64, 2                                            | three passes of a `(2 radius + 1)`-wide box blur, horizontal then vertical each pass, wrapping at the edges |
| `normalize`           | —                                                              | over all channels of all pixels: `(c − min) / (max − min)` (unchanged if `max = min`) |
| `colorize`            | `color_a`: color, `[0, 0, 0]` · `color_b`: color, `[1, 1, 1]`    | `l = (R + G + B) / 3`; result `color_a + (color_b − color_a) × l` |

The image is grayscale (one channel) unless the generator is `noise/curl` or
an enabled `colorize` is present.
