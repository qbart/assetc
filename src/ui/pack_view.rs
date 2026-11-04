//! A pack's Summary, Layout and Treemap views: how the file is occupied, and by what; see
//! docs/features/pack.md#views.

use bevy_egui::egui::{
    Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2,
};

use crate::inspect::format_size;
use crate::pack::{Contents, HEADER_SIZE, KIND_MANIFEST, kind_title};
use crate::theme::{BG_CENTER, BG_HEADER, TEXT, TEXT_WEAK};

/// What the canvas shows of a pack.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum PackView {
    #[default]
    Entries,
    Summary,
    Layout,
    Treemap,
}

impl PackView {
    pub const ALL: [Self; 4] = [Self::Entries, Self::Summary, Self::Layout, Self::Treemap];

    pub fn name(self) -> &'static str {
        match self {
            Self::Entries => "Entries",
            Self::Summary => "Summary",
            Self::Layout => "Layout",
            Self::Treemap => "Treemap",
        }
    }
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

const HEADER: Color32 = rgb(0x78787F);
const CONTENTS: Color32 = rgb(0x5F5F96);
const DATA: Color32 = rgb(0x50A05A);
const MARGIN: f32 = 16.0;

/// The color of an entry kind's bytes.
fn kind_color(kind: u8) -> Color32 {
    rgb(match kind {
        0 => 0x45BFC7,
        1 => 0xEB9645,
        2 => 0x5996EB,
        3 => 0x59C778,
        4 => 0xE078AB,
        5 => 0xE0CC59,
        6 => 0xA68CE0,
        7 => 0x8CC7A6,
        8 => 0xC7A68C,
        9 => 0x5FB3E0,
        KIND_MANIFEST => 0xB578DB,
        _ => 0x838383,
    })
}

/// Draws `view` (not Entries) of `contents`; returns the path of an entry whose block was
/// clicked.
pub fn pack_view(
    ui: &mut Ui,
    rect: Rect,
    contents: &Contents,
    file_name: &str,
    view: PackView,
) -> Option<String> {
    let area = rect.shrink(MARGIN);
    let response = ui.interact(rect, ui.id().with("pack_view"), Sense::click());
    let blocks = match view {
        PackView::Entries => return None,
        PackView::Summary => summary(ui, area, contents, file_name),
        PackView::Layout => layout(ui, area, contents),
        PackView::Treemap => treemap(ui, area, contents),
    };
    let pointer = response.hover_pos()?;
    let &(_, i) = blocks.iter().rev().find(|(r, _)| r.contains(pointer))?;
    let entry = &contents.entries[i];
    let clicked = response.clicked();
    response.on_hover_ui_at_pointer(|ui| {
        let (name, _) = kind_title(entry.kind);
        ui.label(RichText::new(name).color(kind_color(entry.kind)));
        ui.label(RichText::new(&entry.path).color(TEXT));
        let at = if view == PackView::Layout {
            format!("   @{}", entry.offset)
        } else {
            String::new()
        };
        ui.label(RichText::new(format!("{}{at}", format_size(entry.size))).color(TEXT_WEAK));
    });
    clicked.then(|| entry.path.clone())
}

/// A share of `total` as a fraction.
fn share(part: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) as f32
    }
}

fn text(ui: &Ui, at: Pos2, align: Align2, text: impl ToString, size: f32, color: Color32) -> Rect {
    ui.painter()
        .text(at, align, text, FontId::proportional(size), color)
}

/// Title, region bar and one bar per kind; returns no blocks (kinds are not entries).
fn summary(ui: &Ui, area: Rect, c: &Contents, file_name: &str) -> Vec<(Rect, usize)> {
    let painter = ui.painter();
    let title = text(ui, area.min, Align2::LEFT_TOP, file_name, 14.0, TEXT);
    text(
        ui,
        pos2(title.right() + 10.0, area.top() + 1.0),
        Align2::LEFT_TOP,
        format!(
            "APAK v{} · {} · {} entries · alignment {}",
            crate::pack::VERSION,
            format_size(c.bytes),
            c.entries.len(),
            format_size(u64::from(c.alignment))
        ),
        13.0,
        TEXT_WEAK,
    );
    let bar = Rect::from_min_size(
        pos2(area.left(), area.top() + 28.0),
        vec2(area.width(), 16.0),
    );
    let mut x = bar.left();
    for (bytes, color) in [
        (HEADER_SIZE, HEADER),
        (c.toc_bytes, CONTENTS),
        (c.data(), DATA),
        (c.padding(), BG_HEADER),
    ] {
        let w = bar.width() * share(bytes, c.bytes);
        painter.rect_filled(
            Rect::from_min_size(pos2(x, bar.top()), vec2(w, bar.height())),
            0.0,
            color,
        );
        x += w;
    }
    let overhead = share(HEADER_SIZE + c.toc_bytes + c.padding(), c.bytes) * 100.0;
    text(
        ui,
        pos2(area.left(), bar.bottom() + 8.0),
        Align2::LEFT_TOP,
        format!(
            "header {}   contents {}   data {}   padding {} ({overhead:.2}% overhead)",
            format_size(HEADER_SIZE),
            format_size(c.toc_bytes),
            format_size(c.data()),
            format_size(c.padding()),
        ),
        12.0,
        TEXT_WEAK,
    );
    let mut y = bar.bottom() + 40.0;
    let data = c.data();
    for (kind, count, bytes) in c.by_kind() {
        let row = Rect::from_min_size(pos2(area.left(), y), vec2(area.width(), 22.0));
        text(
            ui,
            pos2(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            kind_title(kind).1,
            13.0,
            kind_color(kind),
        );
        let track = Rect::from_min_max(
            pos2(row.left() + 110.0, row.top() + 5.0),
            pos2(row.right() - 148.0, row.bottom() - 5.0),
        );
        if track.width() > 0.0 {
            painter.rect_filled(track, 3.0, BG_HEADER);
            let filled = Rect::from_min_size(
                track.min,
                vec2(
                    (track.width() * share(bytes, data)).max(1.0),
                    track.height(),
                ),
            );
            painter.rect_filled(filled, 3.0, kind_color(kind));
        }
        text(
            ui,
            pos2(row.right(), row.center().y),
            Align2::RIGHT_CENTER,
            format!("{} ({count})", format_size(bytes)),
            12.0,
            TEXT,
        );
        y += 26.0;
    }
    Vec::new()
}

/// A 12 px chip and a name per region and kind; returns where the rows may start.
fn legend(ui: &Ui, area: Rect, c: &Contents) -> f32 {
    let mut items = vec![("header", HEADER), ("contents", CONTENTS)];
    items.extend(
        c.by_kind()
            .into_iter()
            .map(|(kind, ..)| (kind_title(kind).1, kind_color(kind))),
    );
    let mut x = area.left();
    let mut y = area.top();
    for (name, color) in items {
        let width = ui
            .painter()
            .layout_no_wrap(name.to_lowercase(), FontId::proportional(12.0), TEXT)
            .size()
            .x;
        if x + 18.0 + width > area.right() && x > area.left() {
            x = area.left();
            y += 18.0;
        }
        let chip = Rect::from_min_size(pos2(x, y + 2.0), vec2(12.0, 12.0));
        ui.painter().rect_filled(chip, 2.0, color);
        text(
            ui,
            pos2(x + 16.0, y + 8.0),
            Align2::LEFT_CENTER,
            name.to_lowercase(),
            12.0,
            TEXT_WEAK,
        );
        x += 16.0 + width + 14.0;
    }
    y + 28.0
}

/// The memory map: the file cut into rows of equal bytes, read like text.
fn layout(ui: &Ui, area: Rect, c: &Contents) -> Vec<(Rect, usize)> {
    const ROW: f32 = 20.0;
    const GAP: f32 = 4.0;
    const LABELS: f32 = 90.0;
    let top = legend(ui, area, c);
    let fit = (((area.bottom() - top + GAP) / (ROW + GAP)).floor() as u64).max(1);
    let rows = fit.min(c.bytes.div_ceil(64)).max(1);
    let per_row = c.bytes.div_ceil(rows).max(1);
    let x0 = area.left() + LABELS;
    let width = (area.right() - x0).max(1.0);
    let painter = ui.painter();
    let row_rect =
        |r: u64| Rect::from_min_size(pos2(x0, top + r as f32 * (ROW + GAP)), vec2(width, ROW));
    for r in 0..rows {
        let rect = row_rect(r);
        painter.rect_filled(rect, 0.0, BG_HEADER);
        painter.text(
            pos2(area.left(), rect.center().y),
            Align2::LEFT_CENTER,
            format!("{:#010x}", r * per_row),
            FontId::monospace(11.0),
            TEXT_WEAK,
        );
    }
    // Each byte range as one rectangle per row it touches.
    let spans = |start: u64, end: u64| -> Vec<Rect> {
        if end <= start {
            return Vec::new();
        }
        (start / per_row..=(end - 1) / per_row)
            .filter(|&r| r < rows)
            .map(|r| {
                let row = row_rect(r);
                let from = start.max(r * per_row) - r * per_row;
                let to = end.min((r + 1) * per_row) - r * per_row;
                let left = row.left() + width * share(from, per_row);
                let right = (row.left() + width * share(to, per_row)).max(left + 1.0);
                Rect::from_min_max(pos2(left, row.top()), pos2(right, row.bottom()))
            })
            .collect()
    };
    for (start, end, color) in [
        (0, HEADER_SIZE, HEADER),
        (HEADER_SIZE, HEADER_SIZE + c.toc_bytes, CONTENTS),
    ] {
        for rect in spans(start, end) {
            painter.rect_filled(rect, 0.0, color);
        }
    }
    let mut blocks = Vec::new();
    for (i, e) in c.entries.iter().enumerate() {
        let parts = spans(e.offset, e.offset + e.size.max(1));
        for rect in &parts {
            painter.rect_filled(*rect, 0.0, kind_color(e.kind));
            blocks.push((*rect, i));
        }
        if let Some(first) = parts.first() {
            painter.vline(first.left(), first.y_range(), Stroke::new(1.0, BG_CENTER));
        }
    }
    blocks
}

/// The data only: one column per kind, one tile per entry, largest first.
fn treemap(ui: &Ui, area: Rect, c: &Contents) -> Vec<(Rect, usize)> {
    let painter = ui.painter();
    let data = c.data();
    let mut blocks = Vec::new();
    let kinds = c.by_kind();
    // Every kind gets a sliver to point at, however few its bytes.
    const MIN_COLUMN: f32 = 8.0;
    let spread = (area.width() - MIN_COLUMN * kinds.len() as f32).max(0.0);
    let mut x = area.left();
    for (kind, _, bytes) in kinds {
        let column = Rect::from_min_size(
            pos2(x, area.top()),
            vec2(MIN_COLUMN + spread * share(bytes, data), area.height()),
        );
        x = column.right();
        let mut of_kind: Vec<usize> = (0..c.entries.len())
            .filter(|&i| c.entries[i].kind == kind)
            .collect();
        of_kind.sort_by(|&a, &b| c.entries[b].size.cmp(&c.entries[a].size));
        let base = kind_color(kind);
        let mut y = column.top();
        for (n, &i) in of_kind.iter().enumerate() {
            let e = &c.entries[i];
            let h = column.height() * share(e.size, bytes);
            let tile = Rect::from_min_max(
                pos2(column.left(), y),
                pos2(column.right(), (y + h).min(column.bottom())),
            );
            y += h;
            let color = if n % 2 == 1 {
                base.linear_multiply(0.82)
            } else {
                base
            };
            painter.rect_filled(tile, 0.0, color);
            painter.rect_stroke(tile, 0.0, Stroke::new(1.0, BG_CENTER), StrokeKind::Inside);
            // The first tile leaves room for the kind's name.
            let below = if n == 0 && column.width() >= 40.0 {
                16.0
            } else {
                0.0
            };
            if tile.width() >= 46.0 && tile.height() >= 16.0 + below {
                let name = e.path.rsplit('/').next().unwrap_or(&e.path);
                painter.with_clip_rect(tile.intersect(area)).text(
                    tile.min + vec2(3.0, 2.0 + below),
                    Align2::LEFT_TOP,
                    name,
                    FontId::proportional(12.0),
                    BG_CENTER,
                );
            }
            blocks.push((tile, i));
        }
        if column.width() >= 40.0 {
            painter.with_clip_rect(column).text(
                column.min + vec2(3.0, 2.0),
                Align2::LEFT_TOP,
                kind_title(kind).1,
                FontId::proportional(12.0),
                TEXT,
            );
        }
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares() {
        assert_eq!(share(1, 4), 0.25);
        assert_eq!(share(1, 0), 0.0);
        assert_eq!(PackView::ALL.map(PackView::name)[3], "Treemap");
    }
}
