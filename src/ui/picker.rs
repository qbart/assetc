//! File pickers: dropdowns whose list is searched by typing; see
//! docs/features/gui.md#file-pickers.

use std::hash::Hash;

use bevy_egui::egui::{
    self, FontId, Key, Modifiers, RichText, Stroke, TextFormat, Ui, WidgetText, text::LayoutJob,
};

use crate::theme::{ACCENT, TEXT, TEXT_WEAK};

/// How tall the list gets before it scrolls.
const LIST_HEIGHT: f32 = 260.0;

/// What an open picker remembers from frame to frame.
#[derive(Clone, Default)]
struct Search {
    query: String,
    /// The marked row.
    mark: usize,
    /// The frame it was last shown in, to tell when it opens.
    shown: u64,
}

/// A dropdown of `first` (the entry that stands for no file: `None`, `By name: …`) and
/// `files`, searched by typing; `current` is the chosen file, `None` for `first`. Returns the
/// newly chosen entry: `Some(None)` for `first`, `Some(Some(file))` for a file.
pub fn file_picker(
    ui: &mut Ui,
    id_salt: impl Hash + std::fmt::Debug,
    width: f32,
    selected_text: impl Into<WidgetText>,
    first: &str,
    files: &[&str],
    current: Option<&str>,
) -> Option<Option<String>> {
    let id = ui.make_persistent_id(("file_picker", &id_salt));
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id_salt)
        .width(width)
        .truncate()
        // The list scrolls, not the picker: the search field stays in place.
        .height(f32::INFINITY)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .selected_text(selected_text)
        .show_ui(ui, |ui| {
            let frame = ui.ctx().cumulative_frame_nr();
            let mut search = ui.data(|d| d.get_temp::<Search>(id)).unwrap_or_default();
            let opened = search.shown + 1 < frame;
            if opened {
                search = Search::default();
            }
            search.shown = frame;
            let field = ui.add(
                egui::TextEdit::singleline(&mut search.query)
                    .hint_text("Search")
                    .desired_width(ui.available_width()),
            );
            if opened {
                field.request_focus();
            }
            if field.changed() {
                search.mark = 0;
            }
            let rows = matches(&search.query, first, files);
            let (down, up) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                )
            });
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if down {
                search.mark += 1;
            }
            if up {
                search.mark = search.mark.saturating_sub(1);
            }
            search.mark = search.mark.min(rows.len().saturating_sub(1));
            if rows.is_empty() {
                ui.label(RichText::new("No match").color(TEXT_WEAK));
            }
            egui::ScrollArea::vertical()
                .id_salt("file_picker_list")
                .max_height(LIST_HEIGHT)
                .show(ui, |ui| {
                    for (i, row) in rows.iter().enumerate() {
                        let text = row.file.unwrap_or(first);
                        let on = row.file == current;
                        let label = ui.selectable_label(on, highlighted(text, &row.matched));
                        if i == search.mark {
                            let outline = Stroke::new(1.0, ACCENT);
                            ui.painter().rect_stroke(
                                label.rect,
                                2.0,
                                outline,
                                egui::StrokeKind::Inside,
                            );
                            if down || up {
                                label.scroll_to_me(None);
                            }
                        }
                        if label.clicked() || (enter && i == search.mark) {
                            chosen = Some(row.file.map(str::to_owned));
                        }
                    }
                });
            ui.data_mut(|d| d.insert_temp(id, search));
            if chosen.is_some() {
                ui.close();
            }
        });
    chosen
}

/// A row of an open picker.
struct Row<'a> {
    /// `None`: the first entry.
    file: Option<&'a str>,
    /// The characters that matched the search, by their index.
    matched: Vec<usize>,
}

/// The rows for `query`: every entry while it is empty, else the ones that match, best first.
fn matches<'a>(query: &str, first: &str, files: &[&'a str]) -> Vec<Row<'a>> {
    let entries = std::iter::once((None, first)).chain(files.iter().map(|f| (Some(*f), *f)));
    if query.trim().is_empty() {
        return entries
            .map(|(file, _)| Row {
                file,
                matched: Vec::new(),
            })
            .collect();
    }
    let mut scored: Vec<(i32, usize, Row)> = entries
        .filter_map(|(file, text)| {
            let (score, matched) = fuzzy(query, text)?;
            Some((score, text.chars().count(), Row { file, matched }))
        })
        .collect();
    // Stable: equal matches of equal length keep the list's order.
    scored.sort_by_key(|(score, length, _)| (-score, *length));
    scored.into_iter().map(|(.., row)| row).collect()
}

/// `text` with its `matched` characters in `ACCENT`.
fn highlighted(text: &str, matched: &[usize]) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = |color| TextFormat {
        font_id: FontId::proportional(13.0),
        color,
        ..Default::default()
    };
    for (i, c) in text.chars().enumerate() {
        let color = if matched.contains(&i) { ACCENT } else { TEXT };
        job.append(c.encode_utf8(&mut [0; 4]), 0.0, format(color));
    }
    job
}

/// How well `text` matches `query`, and which of its characters (by index) do; `None` when it
/// does not. Every word of `query` has to appear in `text` as its characters in that order,
/// whatever their case; see docs/features/gui.md#file-pickers for what counts.
pub fn fuzzy(query: &str, text: &str) -> Option<(i32, Vec<usize>)> {
    let chars: Vec<char> = text.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| lowercase(*c)).collect();
    // What a match at each character is worth: more at the start of a word and in the
    // file's name.
    let name = chars.iter().rposition(|c| *c == '/').map_or(0, |i| i + 1);
    let worth: Vec<i32> = (0..chars.len())
        .map(|i| {
            let starts = match i.checked_sub(1).map(|p| chars[p]) {
                None => true,
                Some(before) => {
                    matches!(before, '/' | '.' | '_' | '-' | ' ')
                        || (before.is_lowercase() && chars[i].is_uppercase())
                }
            };
            16 + if starts { 12 } else { 0 } + if i >= name { 4 } else { 0 }
        })
        .collect();
    let mut score = 0;
    let mut matched = Vec::new();
    for word in query.split_whitespace() {
        let word: Vec<char> = word.chars().map(lowercase).collect();
        let (word_score, at) = word_match(&word, &lower, &worth)?;
        score += word_score;
        matched.extend(at);
    }
    matched.sort_unstable();
    matched.dedup();
    Some((score, matched))
}

fn lowercase(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// The best way to find `word`'s characters in `text` in order: its score and where they are.
/// A character next to the one before counts 24 more; every character skipped between two
/// counts 1 less.
fn word_match(word: &[char], text: &[char], worth: &[i32]) -> Option<(i32, Vec<usize>)> {
    const NONE: i32 = i32::MIN / 2;
    let (m, n) = (word.len(), text.len());
    if m == 0 || m > n {
        return None;
    }
    // best[j][i]: the best score of the first j + 1 characters with the last one at i;
    // from[j][i]: where the one before it is then.
    let mut best = vec![vec![NONE; n]; m];
    let mut from = vec![vec![0usize; n]; m];
    for i in 0..n {
        if text[i] == word[0] {
            best[0][i] = worth[i];
        }
    }
    for j in 1..m {
        // The best to continue from with a gap, as of the character being looked at.
        let mut gap: (i32, usize) = (NONE, 0);
        for i in 1..n {
            if gap.0 > NONE {
                gap.0 -= 1;
            }
            if i >= 2 && best[j - 1][i - 2] > NONE && best[j - 1][i - 2] - 1 > gap.0 {
                gap = (best[j - 1][i - 2] - 1, i - 2);
            }
            if text[i] != word[j] {
                continue;
            }
            let next_to = best[j - 1][i - 1];
            let (before, at) = if next_to > NONE && next_to + 24 >= gap.0 {
                (next_to + 24, i - 1)
            } else {
                gap
            };
            if before > NONE {
                best[j][i] = before + worth[i];
                from[j][i] = at;
            }
        }
    }
    // The first of equally good ends.
    let (mut at, score) = (0..n)
        .map(|i| (i, best[m - 1][i]))
        .filter(|(_, score)| *score > NONE)
        .reduce(|a, b| if b.1 > a.1 { b } else { a })?;
    let mut matched = vec![0; m];
    for j in (0..m).rev() {
        matched[j] = at;
        at = from[j][at];
    }
    Some((score, matched))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str, files: &[&'static str]) -> Vec<&'static str> {
        matches(query, "None", files)
            .into_iter()
            .map(|row| row.file.unwrap_or("None"))
            .collect()
    }

    #[test]
    fn characters_match_in_order_whatever_their_case() {
        let text = "brick/brick.n.jpg";
        assert!(fuzzy("brkn", text).is_some());
        assert!(fuzzy("BRICK n", text).is_some());
        assert!(fuzzy("jpg bri", text).is_some());
        assert!(fuzzy("nb", text).is_none());
        assert!(fuzzy("brickx", text).is_none());
        assert!(fuzzy("a much longer query than the text", "a").is_none());
        // The characters of the best match: the name's `brick`, not the folder's.
        assert_eq!(fuzzy("brick", text).unwrap().1, [6, 7, 8, 9, 10]);
        assert_eq!(fuzzy("bn", text).unwrap().1, [6, 12]);
    }

    #[test]
    fn better_matches_come_first() {
        let files = [
            "props/barrel_normal.png",
            "brick/brick.ao.jpg",
            "brick/brick.n.jpg",
            "ui/button.png",
            "noise/bn.png",
        ];
        // Next to each other and at a word's start beats spread out.
        assert_eq!(found("bn", &files)[0], "noise/bn.png");
        assert_eq!(found("brick n", &files)[0], "brick/brick.n.jpg");
        assert_eq!(found("but", &files), ["ui/button.png"]);
        // The file's name counts for more than its folders.
        let files = ["rock/moss.png", "moss/rock.png"];
        assert_eq!(found("rock", &files), ["moss/rock.png", "rock/moss.png"]);
        // Equal matches: the shorter path first, else the list's order.
        let files = ["b/long_name.png", "a/long.png", "c/long.png"];
        assert_eq!(
            found("long", &files),
            ["a/long.png", "c/long.png", "b/long_name.png"]
        );
    }

    #[test]
    fn an_empty_search_lists_everything_in_order() {
        let files = ["b.png", "a.png"];
        assert_eq!(found("", &files), ["None", "b.png", "a.png"]);
        assert_eq!(found("  ", &files), ["None", "b.png", "a.png"]);
        // The first entry stays only when it matches too.
        assert_eq!(found("a", &files), ["a.png"]);
        assert_eq!(found("no", &files), ["None"]);
        assert!(found("zzz", &files).is_empty());
    }

    #[test]
    fn matched_characters_are_marked() {
        let job = highlighted("ab.png", &[0, 3]);
        // Runs of one color are one section: a, b., p, ng.
        let colors: Vec<_> = job.sections.iter().map(|s| s.format.color).collect();
        assert_eq!(colors, [ACCENT, TEXT, ACCENT, TEXT]);
        assert_eq!(job.text, "ab.png");
    }
}
