//! HDR environment maps: latitude-longitude OpenEXR images compiled into cubemap textures; see
//! docs/features/hdr.md.

use std::path::Path;

/// Whether `file` is an HDR environment map (`*.exr`, any case).
pub fn is_hdr(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exr"))
}

/// A linear RGBA float image, rows top to bottom.
pub struct Hdr {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[f32; 4]>,
}

/// One cubemap level: six square faces (+X, −X, +Y, −Y, +Z, −Z), `size` texels a side.
pub struct Level {
    pub size: usize,
    pub faces: [Vec<[f32; 4]>; 6],
}

/// Reads the main layer of an OpenEXR file as RGBA (alpha 1 without one, gray as R = G = B).
pub fn read(path: &Path) -> Result<Hdr, String> {
    use imgio::Format;
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let image =
        imgio::file::decode_as(imgio::file::FileFormat::Exr, &bytes).map_err(|e| e.to_string())?;
    if image.width == 0 || image.height == 0 {
        return Err("the image is empty".into());
    }
    let surface = image.surface().map_err(|e| e.to_string())?;
    let floats = |format| {
        imgio::convert::convert_to_buffer(&surface, format)
            .map(|b| {
                b.as_chunks::<4>()
                    .0
                    .iter()
                    .map(|c| f32::from_le_bytes(*c))
                    .collect::<Vec<f32>>()
            })
            .map_err(|e| e.to_string())
    };
    let pixels = match image.format {
        // One channel (luminance): gray.
        Format::R16Float | Format::R32Float => floats(Format::R32Float)?
            .into_iter()
            .map(|v| [v, v, v, 1.0])
            .collect(),
        _ => floats(Format::Rgba32Float)?.as_chunks::<4>().0.to_vec(),
    };
    Ok(Hdr {
        width: image.width as usize,
        height: image.height as usize,
        pixels,
    })
}

/// The face size: the largest power of two not above `height / 2`, at least 1.
pub fn face_size(height: usize) -> usize {
    let half = (height / 2).max(1);
    1 << (usize::BITS - 1 - half.leading_zeros())
}

/// The direction texel `(x, y)` of `face` looks along, as Vulkan picks cube faces.
pub fn direction(face: usize, x: usize, y: usize, size: usize) -> [f32; 3] {
    let at = |c: usize| 2.0 * (c as f32 + 0.5) / size as f32 - 1.0;
    face_direction(face, at(x), at(y))
}

/// The direction of point `(u, v)` (each −1 to 1 across the face, beyond it past an edge) of
/// `face`; see docs/features/hdr.md#cubemap.
fn face_direction(face: usize, u: f32, v: f32) -> [f32; 3] {
    match face {
        0 => [1.0, -v, -u],
        1 => [-1.0, -v, u],
        2 => [u, 1.0, v],
        3 => [u, -1.0, -v],
        4 => [u, -v, 1.0],
        _ => [-u, -v, -1.0],
    }
}

/// The face direction `d` falls on and where: `(face, u, v)`, the inverse of
/// [`face_direction`].
fn face_of(d: [f32; 3]) -> (usize, f32, f32) {
    let [x, y, z] = d;
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    if ax >= ay && ax >= az {
        if x > 0.0 {
            (0, -z / ax, -y / ax)
        } else {
            (1, z / ax, -y / ax)
        }
    } else if ay >= az {
        if y > 0.0 {
            (2, x / ay, z / ay)
        } else {
            (3, x / ay, -z / ay)
        }
    } else if z > 0.0 {
        (4, x / az, -y / az)
    } else {
        (5, -x / az, -y / az)
    }
}

fn normalize(d: [f32; 3]) -> [f32; 3] {
    let length = dot(d, d).sqrt().max(1e-30);
    d.map(|c| c / length)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn lerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|c| a[c] + (b[c] - a[c]) * t)
}

impl Level {
    /// Texel `(x, y)` of `face`; one past an edge is the texel its direction falls in on the
    /// neighbouring face.
    fn texel(&self, face: usize, x: isize, y: isize) -> [f32; 4] {
        let n = self.size as isize;
        if (0..n).contains(&x) && (0..n).contains(&y) {
            return self.faces[face][(y * n + x) as usize];
        }
        let size = self.size as f32;
        let at = |c: isize| 2.0 * (c as f32 + 0.5) / size - 1.0;
        let (face, u, v) = face_of(face_direction(face, at(x), at(y)));
        let index = |c: f32| (((c + 1.0) * 0.5 * size) as isize).clamp(0, n - 1) as usize;
        self.faces[face][index(v) * self.size + index(u)]
    }

    /// The level along direction `d`, bilinearly, across face edges.
    fn sample(&self, d: [f32; 3]) -> [f32; 4] {
        let (face, u, v) = face_of(d);
        let size = self.size as f32;
        let (fx, fy) = ((u + 1.0) * 0.5 * size - 0.5, (v + 1.0) * 0.5 * size - 0.5);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (ax, ay) = (fx - x0, fy - y0);
        let (x, y) = (x0 as isize, y0 as isize);
        let top = lerp(self.texel(face, x, y), self.texel(face, x + 1, y), ax);
        let bottom = lerp(
            self.texel(face, x, y + 1),
            self.texel(face, x + 1, y + 1),
            ax,
        );
        lerp(top, bottom, ay)
    }
}

/// `levels` (a mip chain) along direction `d` at mip `lod`, trilinearly.
fn sample_levels(levels: &[Level], d: [f32; 3], lod: f32) -> [f32; 4] {
    let lod = lod.clamp(0.0, (levels.len() - 1) as f32);
    let below = lod.floor() as usize;
    let t = lod - below as f32;
    let near = levels[below].sample(d);
    match levels.get(below + 1) {
        Some(above) if t > 0.0 => lerp(near, above.sample(d), t),
        _ => near,
    }
}

/// Six faces of `size`, each texel `texel` of its (unit) direction, computed on every core.
fn render(size: usize, texel: impl Fn([f32; 3]) -> [f32; 4] + Sync) -> [Vec<[f32; 4]>; 6] {
    let area = size * size;
    let mut all = vec![[0.0; 4]; 6 * area];
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let chunk = all.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        for (c, out) in all.chunks_mut(chunk).enumerate() {
            let texel = &texel;
            scope.spawn(move || {
                for (j, t) in out.iter_mut().enumerate() {
                    let i = c * chunk + j;
                    let (face, at) = (i / area, i % area);
                    *t = texel(normalize(direction(face, at % size, at / size, size)));
                }
            });
        }
    });
    let mut faces = all.chunks_exact(area).map(<[[f32; 4]]>::to_vec);
    std::array::from_fn(|_| faces.next().unwrap())
}

/// The solid angle texel `(x, y)` of a face of `size` covers, exactly.
fn texel_solid_angle(x: usize, y: usize, size: usize) -> f32 {
    let at = |c: usize| 2.0 * c as f32 / size as f32 - 1.0;
    let corner = |u: f32, v: f32| (u * v).atan2((u * u + v * v + 1.0).sqrt());
    let (u0, u1, v0, v1) = (at(x), at(x + 1), at(y), at(y + 1));
    corner(u0, v0) - corner(u0, v1) - corner(u1, v0) + corner(u1, v1)
}

/// The smallest face size the lighting maps read from.
const MIN_SOURCE: usize = 8;

/// Face size of the irradiance map's source level, at most.
const IRRADIANCE_SOURCE: usize = 32;

/// The 9 real spherical harmonics basis functions at unit direction `d`; see
/// docs/spec/atex.bin.md#sh9.
fn sh_basis([x, y, z]: [f32; 3]) -> [f32; 9] {
    [
        0.282_095,
        0.488_603 * y,
        0.488_603 * z,
        0.488_603 * x,
        1.092_548 * x * y,
        1.092_548 * y * z,
        0.315_392 * (3.0 * z * z - 1.0),
        1.092_548 * x * z,
        0.546_274 * (x * x - y * y),
    ]
}

/// The irradiance (divided by π) that `sh` coefficients give along unit direction `d`.
#[cfg(test)]
pub fn sh_irradiance(sh: &[[f32; 3]; 9], d: [f32; 3]) -> [f32; 3] {
    let basis = sh_basis(d);
    std::array::from_fn(|c| (0..9).map(|i| sh[i][c] * basis[i]).sum())
}

/// What the lighting maps read: the cubemap's mip chain at the skybox's face size, at least
/// [`MIN_SOURCE`]; see docs/features/hdr.md#lighting-maps.
pub fn lighting_source(hdr: &Hdr) -> Vec<Level> {
    hdr.cubemap(face_size(hdr.height).max(MIN_SOURCE))
}

/// The specular map's levels: faces of `min(source size, max_size)` down to 4 px, level `i`
/// prefiltered with GGX for roughness `i / (levels − 1)`; see docs/features/hdr.md#specular.
pub fn specular(source: &[Level], max_size: usize, samples: u32) -> Vec<Level> {
    let full = source[0].size;
    let size = full.min(max_size);
    assert!(size >= MIN_SOURCE && size.is_power_of_two());
    let count = size.trailing_zeros() as usize - 1;
    let first = source
        .iter()
        .find(|l| l.size == size)
        .expect("a power of two");
    let mut levels = vec![Level {
        size,
        faces: first.faces.clone(),
    }];
    let texel_angle = 4.0 * std::f32::consts::PI / (6.0 * (full * full) as f32);
    for i in 1..count {
        let roughness = i as f32 / (count - 1) as f32;
        let lobe = ggx_lobe(roughness * roughness, samples, texel_angle);
        let faces = render(size >> i, |n| {
            let up = if n[2].abs() < 0.999 {
                [0.0, 0.0, 1.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let tangent = normalize(cross(up, n));
            let bitangent = cross(n, tangent);
            let mut sum = [0.0; 4];
            let mut weights = 0.0;
            for &([x, y, z], lod) in &lobe {
                let l = std::array::from_fn(|c| tangent[c] * x + bitangent[c] * y + n[c] * z);
                let color = sample_levels(source, l, lod);
                for c in 0..4 {
                    sum[c] += color[c] * z;
                }
                weights += z;
            }
            sum.map(|c| c / weights)
        });
        levels.push(Level {
            size: size >> i,
            faces,
        });
    }
    levels
}

/// The directions of a GGX lobe of `alpha` around +Z (so `z` is `N·L`), with `N·L > 0`, and
/// the source mip each is read at; Hammersley points, `V = N`.
fn ggx_lobe(alpha: f32, samples: u32, texel_angle: f32) -> Vec<([f32; 3], f32)> {
    let a2 = alpha * alpha;
    (0..samples)
        .filter_map(|k| {
            let (x, y) = (
                k as f32 / samples as f32,
                k.reverse_bits() as f32 / 4_294_967_296.0,
            );
            let phi = std::f32::consts::TAU * x;
            let cos_h = ((1.0 - y) / (1.0 + (a2 - 1.0) * y)).sqrt();
            let sin_h = (1.0 - cos_h * cos_h).max(0.0).sqrt();
            let h = [sin_h * phi.cos(), sin_h * phi.sin(), cos_h];
            let l = [
                2.0 * cos_h * h[0],
                2.0 * cos_h * h[1],
                2.0 * cos_h * cos_h - 1.0,
            ];
            if l[2] <= 0.0 {
                return None;
            }
            let d = cos_h * cos_h * (a2 - 1.0) + 1.0;
            let pdf = a2 / (std::f32::consts::PI * d * d) / 4.0;
            let sample_angle = 1.0 / (samples as f32 * pdf);
            // One level up from the sample's footprint, so neighbouring texels, whose samples
            // fall differently, read alike: no dots from bright suns.
            let lod = (0.5 * (sample_angle / texel_angle).log2() + 1.0).max(0.0);
            Some((l, lod))
        })
        .collect()
}

/// The irradiance map (one level of faces of `size`) and its 9 spherical harmonics
/// coefficients; see docs/features/hdr.md#irradiance.
pub fn irradiance(source: &[Level], size: usize) -> (Level, [[f32; 3]; 9]) {
    let level = source
        .iter()
        .find(|l| l.size <= IRRADIANCE_SOURCE)
        .unwrap_or(&source[source.len() - 1]);
    let m = level.size;
    let texels: Vec<([f32; 3], f32, [f32; 4])> = (0..6)
        .flat_map(|face| (0..m * m).map(move |i| (face, i % m, i / m)))
        .map(|(face, x, y)| {
            (
                normalize(direction(face, x, y, m)),
                texel_solid_angle(x, y, m),
                level.faces[face][y * m + x],
            )
        })
        .collect();
    let faces = render(size, |n| {
        let mut sum = [0.0; 4];
        let mut weights = 0.0;
        for (d, angle, color) in &texels {
            let w = angle * dot(n, *d);
            if w > 0.0 {
                for c in 0..4 {
                    sum[c] += color[c] * w;
                }
                weights += w;
            }
        }
        sum.map(|c| c / weights)
    });
    let mut sh = [[0.0; 3]; 9];
    for (d, angle, color) in &texels {
        for (i, b) in sh_basis(*d).into_iter().enumerate() {
            for c in 0..3 {
                sh[i][c] += color[c] * b * angle;
            }
        }
    }
    // The cosine lobe's bands, divided by π.
    for (i, coefficient) in sh.iter_mut().enumerate() {
        let band = [1.0, 2.0 / 3.0, 0.25][match i {
            0 => 0,
            1..=3 => 1,
            _ => 2,
        }];
        *coefficient = coefficient.map(|c| c * band);
    }
    (Level { size, faces }, sh)
}

impl Hdr {
    /// The image along direction `d`, bilinearly, wrapping across.
    pub fn sample(&self, d: [f32; 3]) -> [f32; 4] {
        let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-12);
        let [x, y, z] = d.map(|c| c / length);
        let s = 0.5 + x.atan2(-z) / std::f32::consts::TAU;
        let t = y.clamp(-1.0, 1.0).acos() / std::f32::consts::PI;
        let fx = s * self.width as f32 - 0.5;
        let fy = (t * self.height as f32 - 0.5).clamp(0.0, (self.height - 1) as f32);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (ax, ay) = (fx - x0, fy - y0);
        let column = |x: f32| (x as isize).rem_euclid(self.width as isize) as usize;
        let row = |y: f32| (y as usize).min(self.height - 1);
        let at = |x: f32, y: f32| self.pixels[row(y) * self.width + column(x)];
        let (p00, p10, p01, p11) = (
            at(x0, y0),
            at(x0 + 1.0, y0),
            at(x0, y0 + 1.0),
            at(x0 + 1.0, y0 + 1.0),
        );
        std::array::from_fn(|c| {
            let top = p00[c] + (p10[c] - p00[c]) * ax;
            let bottom = p01[c] + (p11[c] - p01[c]) * ax;
            top + (bottom - top) * ay
        })
    }

    /// The cubemap's levels: faces of `size`, then each half the size down to 1 × 1, each
    /// texel the average of the 2 × 2 under it. See docs/features/hdr.md#cubemap.
    pub fn cubemap(&self, size: usize) -> Vec<Level> {
        let faces = std::array::from_fn(|face| {
            (0..size * size)
                .map(|i| self.sample(direction(face, i % size, i / size, size)))
                .collect()
        });
        let mut levels = vec![Level { size, faces }];
        while let Some(last) = levels.last()
            && last.size > 1
        {
            let size = last.size / 2;
            let faces = std::array::from_fn(|face| {
                let above = &last.faces[face];
                (0..size * size)
                    .map(|i| {
                        let (x, y) = (i % size * 2, i / size * 2);
                        let at = |dx: usize, dy: usize| above[(y + dy) * last.size + x + dx];
                        let texels = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                        std::array::from_fn(|c| texels.iter().map(|t| t[c]).sum::<f32>() / 4.0)
                    })
                    .collect()
            });
            levels.push(Level { size, faces });
        }
        levels
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn faces_follow_vulkan_and_the_layout() {
        assert_eq!(face_size(1024), 512);
        assert_eq!(face_size(1000), 256);
        assert_eq!(face_size(1), 1);
        // The middle of +X looks along +X, of −Z along −Z.
        assert_eq!(direction(0, 1, 1, 3), [1.0, 0.0, 0.0]);
        assert_eq!(direction(5, 1, 1, 3), [0.0, 0.0, -1.0]);
        // An image whose left half is red and right half blue, with a white top row.
        let (width, height) = (8, 4);
        let pixels = (0..width * height)
            .map(|i| match (i % width, i / width) {
                (_, 0) => [1.0, 1.0, 1.0, 1.0],
                (x, _) if x < width / 2 => [1.0, 0.0, 0.0, 1.0],
                _ => [0.0, 0.0, 1.0, 1.0],
            })
            .collect();
        let hdr = Hdr {
            width,
            height,
            pixels,
        };
        // Up is the top row; +X a quarter right of the middle (blue), −X left (red).
        assert_eq!(hdr.sample([0.0, 1.0, 0.0]), [1.0; 4]);
        assert_eq!(hdr.sample([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(hdr.sample([-1.0, 0.0, 0.0]), [1.0, 0.0, 0.0, 1.0]);
        let levels = hdr.cubemap(4);
        let sizes: Vec<usize> = levels.iter().map(|l| l.size).collect();
        assert_eq!(sizes, [4, 2, 1]);
        assert_eq!(levels[0].faces[0].len(), 16);
    }

    /// An RGB half-float OpenEXR file.
    pub fn write_exr(path: &Path, w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 3]) {
        let halves: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .flat_map(|c| imgio::half::from_f32(c).to_le_bytes())
            .collect();
        let surface = imgio::Surface::new(imgio::Format::Rgb16Float, w, h, &halves).unwrap();
        let bytes = imgio::file::encode(imgio::file::FileFormat::Exr, &surface).unwrap();
        std::fs::write(path, &bytes[..]).unwrap();
    }

    /// A latitude-longitude image of `f` of each pixel's direction.
    fn environment(width: usize, height: usize, f: impl Fn([f32; 3]) -> [f32; 4]) -> Hdr {
        let pixels = (0..width * height)
            .map(|i| {
                let s = ((i % width) as f32 + 0.5) / width as f32;
                let t = ((i / width) as f32 + 0.5) / height as f32;
                let (theta, phi) = (t * std::f32::consts::PI, (s - 0.5) * std::f32::consts::TAU);
                f([
                    theta.sin() * phi.sin(),
                    theta.cos(),
                    -theta.sin() * phi.cos(),
                ])
            })
            .collect();
        Hdr {
            width,
            height,
            pixels,
        }
    }

    fn close(a: [f32; 4], b: [f32; 4], tolerance: f32) -> bool {
        (0..4).all(|c| (a[c] - b[c]).abs() <= tolerance * b[c].abs().max(1.0))
    }

    #[test]
    fn constant_environments_stay_constant() {
        let c = [0.25, 0.5, 3.0, 1.0];
        let hdr = environment(64, 32, |_| c);
        let source = lighting_source(&hdr);
        let specular = specular(&source, 16, 64);
        assert_eq!(
            specular.iter().map(|l| l.size).collect::<Vec<_>>(),
            [16, 8, 4]
        );
        let (irradiance, sh) = irradiance(&source, 4);
        for level in specular.iter().chain([&irradiance]) {
            for texel in level.faces.iter().flatten() {
                assert!(close(*texel, c, 1e-4), "{texel:?}");
            }
        }
        let e = sh_irradiance(&sh, [0.6, 0.0, 0.8]);
        assert!(close([e[0], e[1], e[2], 1.0], c, 1e-3), "{e:?}");
    }

    #[test]
    fn specular_level_0_is_the_source_resampled() {
        let hdr = environment(128, 64, |[x, y, z]| [x + 1.0, y * y, z.abs(), 1.0]);
        let source = lighting_source(&hdr);
        assert_eq!(source[0].size, 32);
        let levels = specular(&source, 16, 32);
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].faces, source[1].faces);
        // Capped at the source; a tiny image is read at 8 px faces.
        assert_eq!(specular(&source, 256, 8)[0].size, 32);
        let tiny = lighting_source(&environment(4, 2, |_| [1.0; 4]));
        assert_eq!(tiny[0].size, 8);
        assert_eq!(specular(&tiny, 256, 8).len(), 2);
    }

    #[test]
    fn irradiance_of_a_white_sky_over_black_ground() {
        let hdr = environment(256, 128, |[_, y, _]| {
            let v = if y > 0.0 { 1.0 } else { 0.0 };
            [v, v, v, 1.0]
        });
        let (level, sh) = irradiance(&lighting_source(&hdr), 8);
        for (face, texels) in level.faces.iter().enumerate() {
            for (i, texel) in texels.iter().enumerate() {
                let n = normalize(direction(face, i % 8, i / 8, 8));
                let expected = (1.0 + n[1]) / 2.0;
                assert!((texel[0] - expected).abs() < 0.02, "{n:?}: {texel:?}");
            }
        }
        for (d, expected) in [([0.0, 1.0, 0.0], 1.0), ([0.0, -1.0, 0.0], 0.0)]
            .into_iter()
            .chain([([1.0, 0.0, 0.0], 0.5), ([0.0, 0.0, -1.0], 0.5)])
        {
            let e = sh_irradiance(&sh, d);
            assert!((e[0] - expected).abs() < 0.02, "{d:?}: {e:?}");
        }
    }

    /// Whether any texel is more than twice the mean of its four neighbours.
    fn has_dots(level: &Level) -> bool {
        let n = level.size as isize;
        (0..6).any(|face| {
            (0..n * n).any(|i| {
                let (x, y) = (i % n, i / n);
                let at = |dx: isize, dy: isize| level.texel(face, x + dx, y + dy)[0];
                let around = (at(-1, 0) + at(1, 0) + at(0, -1) + at(0, 1)) / 4.0;
                at(0, 0) > 2.0 * around
            })
        })
    }

    #[test]
    fn suns_give_smooth_lobes() {
        // One texel 10,000 times brighter than the rest.
        let mut hdr = environment(256, 128, |_| [1.0; 4]);
        hdr.pixels[40 * 256 + 100] = [10_000.0; 4];
        let source = lighting_source(&hdr);
        let levels = specular(&source, 64, 256);
        assert_eq!(levels.len(), 5);
        for (i, level) in levels.iter().enumerate().skip(1) {
            assert!(!has_dots(level), "level {i}");
        }
        assert!(!has_dots(&irradiance(&source, 16).0));
    }

    /// The largest difference between neighbouring texels inside faces, and across face edges.
    fn steps(level: &Level) -> (f32, f32) {
        let n = level.size as isize;
        let (mut inside, mut across) = (0.0f32, 0.0f32);
        for face in 0..6 {
            for y in 0..n {
                for x in 0..n {
                    let here = level.texel(face, x, y);
                    for (nx, ny) in [(x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)] {
                        let there = level.texel(face, nx, ny);
                        let step = (0..3)
                            .map(|c| (here[c] - there[c]).abs())
                            .fold(0.0, f32::max);
                        if (0..n).contains(&nx) && (0..n).contains(&ny) {
                            inside = inside.max(step);
                        } else {
                            across = across.max(step);
                        }
                    }
                }
            }
        }
        (inside, across)
    }

    #[test]
    fn no_seams_at_face_edges() {
        let hdr = environment(256, 128, |[x, y, z]| {
            [1.0 + 0.5 * x, 1.0 + 0.4 * y * z, 1.0 + 0.3 * z, 1.0]
        });
        let source = lighting_source(&hdr);
        let (irradiance, _) = irradiance(&source, 16);
        for level in specular(&source, 32, 128).iter().chain([&irradiance]) {
            let (inside, across) = steps(level);
            assert!(
                across <= inside,
                "{}: {across} across, {inside} inside",
                level.size
            );
        }
    }

    #[test]
    fn faces_wrap_across_edges() {
        for face in 0..6 {
            for (u, v) in [(0.3, -0.7), (-0.9, 0.2)] {
                let (f, u2, v2) = face_of(face_direction(face, u, v));
                assert_eq!(f, face);
                assert!((u - u2).abs() < 1e-6 && (v - v2).abs() < 1e-6);
            }
        }
        // Past +X's right edge is −Z's left column.
        let size = 4;
        let mut level = Level {
            size,
            faces: std::array::from_fn(|_| vec![[0.0; 4]; size * size]),
        };
        level.faces[5][size] = [7.0; 4];
        assert_eq!(level.texel(0, size as isize, 1), [7.0; 4]);
    }

    #[test]
    fn reads_exr_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sky.exr");
        write_exr(&path, 4, 2, |x, _| [x as f32, 2.5, 0.0]);
        let hdr = read(&path).unwrap();
        assert_eq!((hdr.width, hdr.height), (4, 2));
        assert_eq!(hdr.pixels[3], [3.0, 2.5, 0.0, 1.0]);
    }
}
