//! HDR environment maps: reading latitude-longitude OpenEXR images; see docs/features/hdr.md.
//! Cubemaps and lighting maps are the assetc-ibl crate (docs/features/ibl-crate.md).

use std::path::Path;

pub use assetc_ibl::{Hdr, Level, MIN_SOURCE, face_size, irradiance, lighting_source, specular};

/// Whether `file` is an HDR environment map (`*.exr`, any case).
pub fn is_hdr(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("exr"))
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

#[cfg(test)]
pub mod tests {
    use super::*;

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
