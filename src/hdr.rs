//! HDR environment maps: latitude-longitude OpenEXR images compiled into KTX2 cubemaps; see
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

/// Reads the first RGBA layer of an OpenEXR file (alpha 1 without one).
pub fn read(path: &Path) -> Result<Hdr, String> {
    use exr::prelude::*;
    let image = read_first_rgba_layer_from_file(
        path,
        |size, _| Hdr {
            width: size.width(),
            height: size.height(),
            pixels: vec![[0.0, 0.0, 0.0, 1.0]; size.width() * size.height()],
        },
        |hdr: &mut Hdr, at, (r, g, b, a): (f32, f32, f32, f32)| {
            let i = at.y() * hdr.width + at.x();
            hdr.pixels[i] = [r, g, b, a];
        },
    )
    .map_err(|e| e.to_string())?;
    let hdr = image.layer_data.channel_data.pixels;
    if hdr.width == 0 || hdr.height == 0 {
        return Err("the image is empty".into());
    }
    Ok(hdr)
}

/// The face size: the largest power of two not above `height / 2`, at least 1.
pub fn face_size(height: usize) -> usize {
    let half = (height / 2).max(1);
    1 << (usize::BITS - 1 - half.leading_zeros())
}

/// The direction texel `(x, y)` of `face` looks along, as Vulkan picks cube faces.
pub fn direction(face: usize, x: usize, y: usize, size: usize) -> [f32; 3] {
    let u = 2.0 * (x as f32 + 0.5) / size as f32 - 1.0;
    let v = 2.0 * (y as f32 + 0.5) / size as f32 - 1.0;
    match face {
        0 => [1.0, -v, -u],
        1 => [-1.0, -v, u],
        2 => [u, 1.0, v],
        3 => [u, -1.0, -v],
        4 => [u, -v, 1.0],
        _ => [-u, -v, -1.0],
    }
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
mod tests {
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

    #[test]
    fn reads_exr_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sky.exr");
        exr::prelude::write_rgb_file(&path, 4, 2, |x, _| (x as f32, 2.5f32, 0.0f32)).unwrap();
        let hdr = read(&path).unwrap();
        assert_eq!((hdr.width, hdr.height), (4, 2));
        assert_eq!(hdr.pixels[3], [3.0, 2.5, 0.0, 1.0]);
    }
}
