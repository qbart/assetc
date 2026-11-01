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
frequency = 4
octaves = 5
lacunarity = 2
gain = 0.5

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

| `type`               | Parameters (key: type, range, default)                             |
|----------------------|--------------------------------------------------------------------|
| `checkerboard`       | `cells`: int 1–256, 8 · `color_a`: gray, 0 · `color_b`: gray, 1      |
| `noise/uniform`      | `seed`: int 0–4294967295, 0                                        |
| `noise/value`        | `seed`, `frequency`: int 1–256, 8                                  |
| `noise/perlin`       | `seed`, `frequency`: int 1–256, 8                                  |
| `noise/simplex`      | `seed`, `frequency`: int 1–256, 8                                  |
| `noise/fbm`          | `seed`, `basis`: choice `value` `perlin` `simplex`, `perlin` · `frequency`: 4 · `octaves`: int 1–8, 5 · `lacunarity`: int 2–4, 2 · `gain`: float 0–1, 0.5 |
| `noise/voronoi`      | `seed`, `frequency`: 8 · `jitter`: float 0–1, 1 · `metric`: choice `euclidean` `manhattan` `chebyshev`, `euclidean` · `output`: choice `f1` `edge` `cell`, `f1` |
| `noise/perlin-worley`| `seed`, `frequency`: 4 · `octaves`: int 1–8, 4                     |
| `noise/musgrave`     | `seed`, `frequency`: 4 · `octaves`: int 1–8, 6 · `lacunarity`: int 2–4, 2 · `dimension`: float 0–2, 1 |
| `noise/ridged`       | `seed`, `frequency`: 4 · `octaves`: int 1–8, 6 · `lacunarity`: int 2–4, 2 · `dimension`: float 0–2, 1 · `offset`: float 0–2, 1 · `gain`: float 0–4, 2 |
| `noise/curl`         | `seed`, `frequency`: 4 · `octaves`: int 1–8, 2                     |

(`seed` is always int 0–4294967295, default 0; `frequency` int 1–256.)

## Generated image

`size × size` pixels, each an RGB value in `0..1`, stored as 8-bit
`round(c × 255)` with `A = 255`. Every generator except `noise/curl` produces
gray (`R = G = B`). Everything tiles seamlessly when a lattice has a whole
number of cells across the image (all noises; `checkerboard` when `cells` is
even).

Pixel `(px, py)` samples at `u = (px + 0.5) / size`, `v = (py + 0.5) / size`.
A lattice of period `p` (cells across the image) uses coordinates
`(x, y) = (u × p, v × p)` and takes lattice indices modulo `p`.

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

### Basic noises (one lattice of period `p`, seed `s`)

- **value(p, s)**: lattice values `unit(hash(i mod p, j mod p, s))`,
  interpolated with the quintic fade `f(t) = 6t⁵ − 15t⁴ + 10t³`. Range 0–1.
- **perlin(p, s)**: gradient at each lattice point with angle
  `2π × unit(hash(i mod p, j mod p, s))`; the four corner dot products
  (gradient · offset to the sample) are interpolated with the fade, giving `n`
  in ±√½; the result is `clamp(0.5 + n × √½, 0, 1)`.
- **simplex(p, s)**: 4D simplex noise (Gustavson's algorithm: skew
  `F4 = (√5 − 1)/4`, unskew `G4 = (5 − √5)/20`, radius² 0.6, scale 27) sampled
  on a torus so it tiles: with `r = p / 2π`, the point is
  `(r cos 2πu, r sin 2πu, r cos 2πv, r sin 2πv)`; each simplex corner's
  gradient is entry `hash4(i, j, k, l, s) mod 32` of the 32 vectors that are
  the permutations of `(0, ±1, ±1, ±1)` (zero in each of the four positions,
  ordered by zero position then signs `+++`, `++−`, `+−+`, `+−−`, `−++`, …).
  Result `clamp(0.5 + 0.5 n, 0, 1)`.
- **worley(p, s)**: one feature point per cell at
  `cell + (unit(hash(i, j, s)), unit(hash(i, j, s ^ 0x5bd1e995)))` (`i, j`
  modulo `p`); the Euclidean distance to the nearest feature point among the
  3 × 3 neighbouring cells, clamped to 0–1.

A *signed* basic noise is `2 × noise − 1`.

### Generators

- **checkerboard**: `color_a` where `floor(u × cells) + floor(v × cells)` is
  even, else `color_b`.
- **noise/uniform**: `unit(hash(px, py, seed))` per pixel.
- **noise/value**, **noise/perlin**, **noise/simplex**: the basic noise with
  `p = frequency`, `s = seed`.
- **noise/fbm**: octave `o = 0 … octaves−1` uses `p = frequency × lacunarity^o`,
  `s = seed + o` (wrapping), weight `gain^o`; result
  `Σ gain^o × basis(p, s) / Σ gain^o`.
- **noise/voronoi**: feature point of cell `(i, j)` at
  `cell + 0.5 + jitter × (unit(hash(i, j, seed)) − 0.5, unit(hash(i, j, seed ^ 0x5bd1e995)) − 0.5)`
  (`i, j` modulo `p = frequency`), searched over the 5 × 5 neighbouring cells
  with the chosen metric (`euclidean` √(dx²+dy²), `manhattan` |dx|+|dy|,
  `chebyshev` max(|dx|, |dy|)), distances in cells. `f1`: nearest distance;
  `edge`: second nearest minus nearest; `cell`: `unit(hash(i, j, seed ^ 0x2545F491))`
  of the nearest point's cell. Clamped to 0–1.
- **noise/perlin-worley**: `P` = fbm with basis perlin (`frequency`,
  `octaves`, lacunarity 2, gain 0.5, `seed`); `W` = `1 −` (worley at periods
  `frequency × 2^o`, seeds `seed + 100 + o`, weights `0.5^o`, `o = 0, 1, 2`,
  weighted average); result `clamp((P − W + 1) / (2 − W), 0, 1)` (Perlin
  remapped from `[W − 1, 1]`, as for volumetric clouds).
- **noise/musgrave** (multifractal): with signed perlin `n_o` at
  `p = frequency × lacunarity^o`, `s = seed + o`, and `w_o = lacunarity^(−dimension × o)`:
  `∏ (1 + n_o w_o) / ∏ (1 + w_o)`.
- **noise/ridged** (ridged multifractal): `sig_0 = (offset − |n_0|)²`,
  `sum = sig_0`, `max = offset²`; for `o ≥ 1`:
  `weight = clamp(sig_(o−1) × gain, 0, 1)`, `sig_o = (offset − |n_o|)² × weight`,
  `sum += sig_o × w_o`, `max += offset² × w_o`; result `clamp(sum / max, 0, 1)`
  (`n_o`, `w_o` as for musgrave).
- **noise/curl**: potential `ψ` = signed fbm with basis perlin (`frequency`,
  `octaves`, lacunarity 2, gain 0.5, `seed`); its gradient by central
  differences one pixel apart (wrapping at the edges), in pixels;
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
