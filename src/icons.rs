use std::sync::LazyLock;

use bevy_egui::egui::{Color32, Image, ImageSource, load::Bytes, vec2};

/// Lucide icons vendored in assets/icons, see docs/features/gui.md.
#[derive(Clone, Copy)]
pub enum Icon {
    Aperture,
    Box,
    Check,
    ChevronDown,
    ChevronRight,
    CircleSmall,
    Cylinder,
    Eye,
    EyeOff,
    File,
    Files,
    Folder,
    FolderOpen,
    Globe,
    GripHorizontal,
    GripVertical,
    Grid3x3,
    Hammer,
    Image,
    ImagePlay,
    Info,
    Loader,
    Palette,
    Plus,
    RefreshCw,
    SlidersVertical,
    SunMedium,
    Torus,
    Trash2,
    Type,
    Wrench,
}

// Same order as `Icon`.
const SVGS: [(&str, &str); 31] = [
    ("aperture", include_str!("../assets/icons/aperture.svg")),
    ("box", include_str!("../assets/icons/box.svg")),
    ("check", include_str!("../assets/icons/check.svg")),
    (
        "chevron-down",
        include_str!("../assets/icons/chevron-down.svg"),
    ),
    (
        "chevron-right",
        include_str!("../assets/icons/chevron-right.svg"),
    ),
    (
        "circle-small",
        include_str!("../assets/icons/circle-small.svg"),
    ),
    ("cylinder", include_str!("../assets/icons/cylinder.svg")),
    ("eye", include_str!("../assets/icons/eye.svg")),
    ("eye-off", include_str!("../assets/icons/eye-off.svg")),
    ("file", include_str!("../assets/icons/file.svg")),
    ("files", include_str!("../assets/icons/files.svg")),
    ("folder", include_str!("../assets/icons/folder.svg")),
    (
        "folder-open",
        include_str!("../assets/icons/folder-open.svg"),
    ),
    ("globe", include_str!("../assets/icons/globe.svg")),
    (
        "grip-horizontal",
        include_str!("../assets/icons/grip-horizontal.svg"),
    ),
    (
        "grip-vertical",
        include_str!("../assets/icons/grip-vertical.svg"),
    ),
    ("grid-3x3", include_str!("../assets/icons/grid-3x3.svg")),
    ("hammer", include_str!("../assets/icons/hammer.svg")),
    ("image", include_str!("../assets/icons/image.svg")),
    ("image-play", include_str!("../assets/icons/image-play.svg")),
    ("info", include_str!("../assets/icons/info.svg")),
    ("loader", include_str!("../assets/icons/loader.svg")),
    ("palette", include_str!("../assets/icons/palette.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    ("refresh-cw", include_str!("../assets/icons/refresh-cw.svg")),
    (
        "sliders-vertical",
        include_str!("../assets/icons/sliders-vertical.svg"),
    ),
    ("sun-medium", include_str!("../assets/icons/sun-medium.svg")),
    ("torus", include_str!("../assets/icons/torus.svg")),
    ("trash-2", include_str!("../assets/icons/trash-2.svg")),
    ("type", include_str!("../assets/icons/type.svg")),
    ("wrench", include_str!("../assets/icons/wrench.svg")),
];

// Rendered white so `Image::tint` can color them.
static WHITE: LazyLock<Vec<Bytes>> = LazyLock::new(|| {
    SVGS.iter()
        .map(|(_, svg)| Bytes::Shared(svg.replace("currentColor", "white").into_bytes().into()))
        .collect()
});

impl Icon {
    pub fn image(self, size: f32, tint: Color32) -> Image<'static> {
        let (name, _) = SVGS[self as usize];
        Image::new(ImageSource::Bytes {
            uri: format!("bytes://lucide/{name}.svg").into(),
            bytes: WHITE[self as usize].clone(),
        })
        .tint(tint)
        .fit_to_exact_size(vec2(size, size))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svgs_match_enum_order() {
        let names: Vec<_> = SVGS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names[Icon::Info as usize], "info");
        assert_eq!(names[Icon::Image as usize], "image");
        assert_eq!(names[Icon::Type as usize], "type");
        assert_eq!(names[Icon::Wrench as usize], "wrench");
        for (name, svg) in SVGS {
            assert!(svg.contains(&format!("lucide-{name}\"")), "{name}");
        }
    }
}
