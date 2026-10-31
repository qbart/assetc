use std::sync::LazyLock;

use bevy_egui::egui::{Color32, Image, ImageSource, load::Bytes, vec2};

/// Lucide icons vendored in assets/icons, see docs/features/gui.md.
#[derive(Clone, Copy)]
pub enum Icon {
    ChevronDown,
    ChevronRight,
    File,
    Folder,
    FolderOpen,
    Hammer,
    Image,
    Info,
    Plus,
    RefreshCw,
    Settings,
}

// Same order as `Icon`.
const SVGS: [(&str, &str); 11] = [
    (
        "chevron-down",
        include_str!("../assets/icons/chevron-down.svg"),
    ),
    (
        "chevron-right",
        include_str!("../assets/icons/chevron-right.svg"),
    ),
    ("file", include_str!("../assets/icons/file.svg")),
    ("folder", include_str!("../assets/icons/folder.svg")),
    (
        "folder-open",
        include_str!("../assets/icons/folder-open.svg"),
    ),
    ("hammer", include_str!("../assets/icons/hammer.svg")),
    ("image", include_str!("../assets/icons/image.svg")),
    ("info", include_str!("../assets/icons/info.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    ("refresh-cw", include_str!("../assets/icons/refresh-cw.svg")),
    ("settings", include_str!("../assets/icons/settings.svg")),
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
        assert_eq!(names[Icon::Settings as usize], "settings");
        for (name, svg) in SVGS {
            assert!(svg.contains(&format!("lucide-{name}\"")), "{name}");
        }
    }
}
