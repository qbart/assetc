//! Keyboard navigation in the file trees, see docs/features/gui.md#file-browser.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bevy_egui::egui::Key;

use crate::tree::Node;

/// A row the tree shows, in screen order.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    /// `Some(open)` for folders.
    pub folder: Option<bool>,
}

/// Whether `node` is a sidecar listed below its file instead of in name order.
pub fn nested(nodes: &[Node], node: &Node) -> bool {
    node.children.is_none()
        && node
            .name
            .strip_suffix(".amod")
            .is_some_and(|main| nodes.iter().any(|n| n.children.is_none() && n.name == main))
}

/// The sidecar listed right below `file`, if any.
pub fn sidecar_of<'a>(nodes: &'a [Node], file: &Node) -> Option<&'a Node> {
    if file.children.is_some() {
        return None;
    }
    nodes
        .iter()
        .find(|n| n.children.is_none() && n.name.strip_suffix(".amod") == Some(file.name.as_str()))
}

/// The rows the tree draws, in order: folded folders' contents skipped, sidecars below their
/// files.
pub fn visible_rows(nodes: &[Node], open: &HashSet<PathBuf>) -> Vec<Row> {
    let mut rows = Vec::new();
    collect(nodes, open, 0, &mut rows);
    rows
}

fn collect(nodes: &[Node], open: &HashSet<PathBuf>, depth: usize, rows: &mut Vec<Row>) {
    for node in nodes.iter().filter(|n| !nested(nodes, n)) {
        let is_open = open.contains(&node.path);
        rows.push(Row {
            path: node.path.clone(),
            depth,
            folder: node.children.is_some().then_some(is_open),
        });
        if let Some(sidecar) = sidecar_of(nodes, node) {
            rows.push(Row {
                path: sidecar.path.clone(),
                depth: depth + 1,
                folder: None,
            });
        }
        if let Some(children) = &node.children
            && is_open
        {
            collect(children, open, depth + 1, rows);
        }
    }
}

/// What a navigation key does.
#[derive(Debug, PartialEq)]
pub enum Move {
    Select(PathBuf),
    /// Opens (`true`) or closes a folder.
    Fold(PathBuf, bool),
}

/// The navigation keys, in the order they are checked.
pub const KEYS: [Key; 9] = [
    Key::ArrowUp,
    Key::ArrowDown,
    Key::ArrowLeft,
    Key::ArrowRight,
    Key::Home,
    Key::End,
    Key::PageUp,
    Key::PageDown,
    Key::Enter,
];

/// What `key` does with `selected` the selected path (when it is in this tree) and `page` rows
/// per page; `None` when it does nothing.
pub fn navigate(rows: &[Row], selected: Option<&Path>, key: Key, page: usize) -> Option<Move> {
    let last = rows.len().checked_sub(1)?;
    let current = selected.and_then(|s| rows.iter().position(|r| r.path == s));
    let target = match (key, current) {
        (Key::ArrowUp, None) | (Key::End, _) => last,
        (Key::ArrowDown, None) | (Key::Home, _) => 0,
        (Key::PageUp, None) => last,
        (Key::PageDown, None) => 0,
        (Key::ArrowUp, Some(i)) => i.checked_sub(1)?,
        (Key::ArrowDown, Some(i)) => (i < last).then_some(i + 1)?,
        (Key::PageUp, Some(i)) => i.saturating_sub(page),
        (Key::PageDown, Some(i)) => (i + page).min(last),
        (Key::ArrowLeft, Some(i)) => {
            let row = &rows[i];
            if row.folder == Some(true) {
                return Some(Move::Fold(row.path.clone(), false));
            }
            rows[..i].iter().rposition(|r| r.depth < row.depth)?
        }
        (Key::ArrowRight, Some(i)) => match rows[i].folder {
            Some(false) => return Some(Move::Fold(rows[i].path.clone(), true)),
            Some(true) => rows
                .get(i + 1)
                .filter(|r| r.depth > rows[i].depth)
                .map(|_| i + 1)?,
            None => return None,
        },
        (Key::Enter, Some(i)) => {
            let open = rows[i].folder?;
            return Some(Move::Fold(rows[i].path.clone(), !open));
        }
        _ => return None,
    };
    (Some(target) != current).then(|| Move::Select(rows[target].path.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> Node {
        Node {
            name: path.rsplit('/').next().unwrap().into(),
            path: path.into(),
            children: None,
        }
    }

    fn folder(path: &str, children: Vec<Node>) -> Node {
        Node {
            children: Some(children),
            ..file(path)
        }
    }

    /// `a/` (open) holding `a/b/` (closed, with `a/b/c`) and `a/m.glb` with its sidecar, then
    /// `z.png`.
    fn sample() -> (Vec<Node>, HashSet<PathBuf>) {
        let nodes = vec![
            folder(
                "a",
                vec![
                    folder("a/b", vec![file("a/b/c")]),
                    file("a/m.glb"),
                    file("a/m.glb.amod"),
                ],
            ),
            file("z.png"),
        ];
        (nodes, HashSet::from([PathBuf::from("a")]))
    }

    fn select(path: &str) -> Option<Move> {
        Some(Move::Select(path.into()))
    }

    #[test]
    fn rows_skip_folded_folders_and_nest_sidecars() {
        let (nodes, open) = sample();
        let rows = visible_rows(&nodes, &open);
        let listed: Vec<_> = rows
            .iter()
            .map(|r| (r.path.to_str().unwrap(), r.depth, r.folder))
            .collect();
        assert_eq!(
            listed,
            [
                ("a", 0, Some(true)),
                ("a/b", 1, Some(false)),
                ("a/m.glb", 1, None),
                ("a/m.glb.amod", 2, None),
                ("z.png", 0, None),
            ]
        );
    }

    #[test]
    fn keys_move_the_selection() {
        let (nodes, open) = sample();
        let rows = visible_rows(&nodes, &open);
        let go = |selected: Option<&str>, key| navigate(&rows, selected.map(Path::new), key, 2);
        assert_eq!(go(None, Key::ArrowDown), select("a"));
        assert_eq!(go(None, Key::ArrowUp), select("z.png"));
        assert_eq!(go(Some("a"), Key::ArrowUp), None);
        assert_eq!(go(Some("z.png"), Key::ArrowDown), None);
        assert_eq!(go(Some("a/b"), Key::ArrowDown), select("a/m.glb"));
        assert_eq!(go(Some("a/b"), Key::End), select("z.png"));
        assert_eq!(go(Some("a/b"), Key::Home), select("a"));
        assert_eq!(go(Some("a"), Key::PageDown), select("a/m.glb"));
        assert_eq!(go(Some("a/m.glb"), Key::PageDown), select("z.png"));
        assert_eq!(go(Some("a/m.glb"), Key::PageUp), select("a"));
        // Not shown (inside a folded folder): as if nothing were selected.
        assert_eq!(go(Some("a/b/c"), Key::ArrowDown), select("a"));
    }

    #[test]
    fn left_and_right_fold_and_climb() {
        let (nodes, open) = sample();
        let rows = visible_rows(&nodes, &open);
        let go = |selected: &str, key| navigate(&rows, Some(Path::new(selected)), key, 1);
        assert_eq!(go("a", Key::ArrowLeft), Some(Move::Fold("a".into(), false)));
        assert_eq!(go("a/b", Key::ArrowLeft), select("a"));
        assert_eq!(go("a/m.glb.amod", Key::ArrowLeft), select("a/m.glb"));
        assert_eq!(go("z.png", Key::ArrowLeft), None);
        assert_eq!(
            go("a/b", Key::ArrowRight),
            Some(Move::Fold("a/b".into(), true))
        );
        assert_eq!(go("a", Key::ArrowRight), select("a/b"));
        assert_eq!(go("z.png", Key::ArrowRight), None);
        assert_eq!(go("a/b", Key::Enter), Some(Move::Fold("a/b".into(), true)));
        assert_eq!(go("a", Key::Enter), Some(Move::Fold("a".into(), false)));
        assert_eq!(go("z.png", Key::Enter), None);
        // An open folder without rows: Right does nothing.
        let empty = vec![folder("e", vec![])];
        let open = HashSet::from([PathBuf::from("e")]);
        let rows = visible_rows(&empty, &open);
        assert_eq!(
            navigate(&rows, Some(Path::new("e")), Key::ArrowRight, 1),
            None
        );
    }
}
