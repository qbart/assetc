use std::sync::LazyLock;

use bevy_egui::egui::{Color32, Image, ImageSource, load::Bytes, vec2};

/// Lucide icons vendored in assets/icons, see docs/features/gui.md.
#[derive(Clone, Copy)]
pub enum Icon {
    Aperture,
    Box,
    BoxChecker,
    BoxShaded,
    Boxes,
    Braces,
    Bug,
    CircleDashed,
    CircleDot,
    BringToFront,
    Check,
    ChevronDown,
    ChevronRight,
    CircleSmall,
    Cone,
    Crosshair,
    Cylinder,
    Eraser,
    Eye,
    EyeOff,
    EyeOutline,
    File,
    Files,
    Film,
    FlipHorizontal2,
    Folder,
    FolderOpen,
    Gem,
    Globe,
    GraphStar,
    GripHorizontal,
    GripVertical,
    Grid3x3,
    Group,
    Hammer,
    Image,
    ImagePlay,
    Images,
    Info,
    Database,
    LayoutGrid,
    ListTree,
    Map,
    Link2,
    Loader,
    MapPin,
    Move3d,
    Package,
    PaintBucket,
    Paintbrush,
    Palette,
    Pause,
    PenLine,
    Play,
    Plus,
    RefreshCw,
    Rotate3d,
    RotateCcw,
    Scan,
    Settings,
    Settings2,
    Shapes,
    SlidersVertical,
    Spline,
    SquaresUnite,
    SunMedium,
    SwatchBook,
    Torus,
    Trash2,
    Type,
    Unlink2,
    Wrench,
    X,
}

// Same order as `Icon`.
const SVGS: [(&str, &str); 73] = [
    ("aperture", include_str!("../assets/icons/aperture.svg")),
    ("box", include_str!("../assets/icons/box.svg")),
    (
        "box-checker",
        include_str!("../assets/icons/box-checker.svg"),
    ),
    ("box-shaded", include_str!("../assets/icons/box-shaded.svg")),
    ("boxes", include_str!("../assets/icons/boxes.svg")),
    ("braces", include_str!("../assets/icons/braces.svg")),
    ("bug", include_str!("../assets/icons/bug.svg")),
    (
        "circle-dashed",
        include_str!("../assets/icons/circle-dashed.svg"),
    ),
    ("circle-dot", include_str!("../assets/icons/circle-dot.svg")),
    (
        "bring-to-front",
        include_str!("../assets/icons/bring-to-front.svg"),
    ),
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
    ("cone", include_str!("../assets/icons/cone.svg")),
    ("crosshair", include_str!("../assets/icons/crosshair.svg")),
    ("cylinder", include_str!("../assets/icons/cylinder.svg")),
    ("eraser", include_str!("../assets/icons/eraser.svg")),
    ("eye", include_str!("../assets/icons/eye.svg")),
    ("eye-off", include_str!("../assets/icons/eye-off.svg")),
    (
        "eye-outline",
        include_str!("../assets/icons/eye-outline.svg"),
    ),
    ("file", include_str!("../assets/icons/file.svg")),
    ("files", include_str!("../assets/icons/files.svg")),
    ("film", include_str!("../assets/icons/film.svg")),
    (
        "flip-horizontal-2",
        include_str!("../assets/icons/flip-horizontal-2.svg"),
    ),
    ("folder", include_str!("../assets/icons/folder.svg")),
    (
        "folder-open",
        include_str!("../assets/icons/folder-open.svg"),
    ),
    ("gem", include_str!("../assets/icons/gem.svg")),
    ("globe", include_str!("../assets/icons/globe.svg")),
    ("graph-star", include_str!("../assets/icons/graph-star.svg")),
    (
        "grip-horizontal",
        include_str!("../assets/icons/grip-horizontal.svg"),
    ),
    (
        "grip-vertical",
        include_str!("../assets/icons/grip-vertical.svg"),
    ),
    ("grid-3x3", include_str!("../assets/icons/grid-3x3.svg")),
    ("group", include_str!("../assets/icons/group.svg")),
    ("hammer", include_str!("../assets/icons/hammer.svg")),
    ("image", include_str!("../assets/icons/image.svg")),
    ("image-play", include_str!("../assets/icons/image-play.svg")),
    ("images", include_str!("../assets/icons/images.svg")),
    ("info", include_str!("../assets/icons/info.svg")),
    ("database", include_str!("../assets/icons/database.svg")),
    (
        "layout-grid",
        include_str!("../assets/icons/layout-grid.svg"),
    ),
    ("list-tree", include_str!("../assets/icons/list-tree.svg")),
    ("map", include_str!("../assets/icons/map.svg")),
    ("link-2", include_str!("../assets/icons/link-2.svg")),
    ("loader", include_str!("../assets/icons/loader.svg")),
    ("map-pin", include_str!("../assets/icons/map-pin.svg")),
    ("move-3d", include_str!("../assets/icons/move-3d.svg")),
    ("package", include_str!("../assets/icons/package.svg")),
    (
        "paint-bucket",
        include_str!("../assets/icons/paint-bucket.svg"),
    ),
    ("paintbrush", include_str!("../assets/icons/paintbrush.svg")),
    ("palette", include_str!("../assets/icons/palette.svg")),
    ("pause", include_str!("../assets/icons/pause.svg")),
    ("pen-line", include_str!("../assets/icons/pen-line.svg")),
    ("play", include_str!("../assets/icons/play.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    ("refresh-cw", include_str!("../assets/icons/refresh-cw.svg")),
    ("rotate-3d", include_str!("../assets/icons/rotate-3d.svg")),
    ("rotate-ccw", include_str!("../assets/icons/rotate-ccw.svg")),
    ("scan", include_str!("../assets/icons/scan.svg")),
    ("settings", include_str!("../assets/icons/settings.svg")),
    ("settings-2", include_str!("../assets/icons/settings-2.svg")),
    ("shapes", include_str!("../assets/icons/shapes.svg")),
    (
        "sliders-vertical",
        include_str!("../assets/icons/sliders-vertical.svg"),
    ),
    ("spline", include_str!("../assets/icons/spline.svg")),
    (
        "squares-unite",
        include_str!("../assets/icons/squares-unite.svg"),
    ),
    ("sun-medium", include_str!("../assets/icons/sun-medium.svg")),
    (
        "swatch-book",
        include_str!("../assets/icons/swatch-book.svg"),
    ),
    ("torus", include_str!("../assets/icons/torus.svg")),
    ("trash-2", include_str!("../assets/icons/trash-2.svg")),
    ("type", include_str!("../assets/icons/type.svg")),
    ("unlink-2", include_str!("../assets/icons/unlink-2.svg")),
    ("wrench", include_str!("../assets/icons/wrench.svg")),
    ("x", include_str!("../assets/icons/x.svg")),
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
        assert_eq!(names[Icon::X as usize], "x");
        for (name, svg) in SVGS {
            assert!(svg.contains(&format!("lucide-{name}\"")), "{name}");
        }
    }
}
