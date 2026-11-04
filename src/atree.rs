//! `.atree` trees: one root node with nested children, each with a position and data; see
//! docs/spec/atree.md and docs/features/trees.md. A tree is held as a [`Graph`] whose first
//! node is the root and whose edges link each parent to its children, in order.

use std::path::Path;

use serde_json::{Map, Value};

use crate::agraph::{Edge, Graph, Node, parse_node, write_node};

pub const DIALECT: &str = "atree/1";

/// Whether `file` is a tree (`*.atree`, any case).
pub fn is_tree(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("atree"))
}

/// One root `root` at the origin; see docs/features/trees.md#creating.
pub fn new() -> Graph {
    Graph {
        nodes: vec![Node {
            label: "Root".into(),
            ..Node::new("root".into(), 0.0, 0.0)
        }],
        ..Graph::default()
    }
}

/// Reads a tree, its nodes depth first.
pub fn parse(text: &str) -> Result<Graph, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
    let Value::Object(mut top) = value else {
        return Err("not a JSON object".into());
    };
    match top.shift_remove("$dialect") {
        Some(Value::String(d)) if d == DIALECT => {}
        Some(Value::String(d)) => match d.strip_prefix("atree/").map(str::parse::<u32>) {
            Some(Ok(major)) if major > 1 => {
                return Err(format!("$dialect {d:?} is newer than {DIALECT}"));
            }
            _ => return Err(format!("unknown $dialect {d:?}")),
        },
        _ => return Err("no $dialect".into()),
    }
    let structs = crate::structs::take_attachments(&mut top, "structs")?;
    let node_structs = crate::structs::take_attachments(&mut top, "node_structs")?;
    let values = crate::structs::take_values(&mut top)?;
    let root = top.shift_remove("root").ok_or("no root")?;
    let mut graph = Graph {
        structs,
        node_structs,
        values,
        extra: top,
        ..Graph::default()
    };
    add(&mut graph, root, None)?;
    Ok(graph)
}

/// Adds a node and, under it, its children.
fn add(graph: &mut Graph, value: Value, parent: Option<usize>) -> Result<(), String> {
    let Value::Object(mut o) = value else {
        return Err(format!("node {}: not an object", graph.nodes.len() + 1));
    };
    let children = match o.shift_remove("children") {
        None => Vec::new(),
        Some(Value::Array(items)) => items,
        Some(_) => {
            return Err(format!(
                "node {}: children is not an array",
                graph.nodes.len() + 1
            ));
        }
    };
    let node =
        parse_node(Value::Object(o)).map_err(|e| format!("node {}: {e}", graph.nodes.len() + 1))?;
    let index = graph.nodes.len();
    if let Some(p) = parent {
        let from = graph.nodes[p].id.clone();
        graph.edges.push(Edge::new(from, node.id.clone()));
    }
    graph.nodes.push(node);
    for child in children {
        add(graph, child, Some(index))?;
    }
    Ok(())
}

/// The written form; see docs/spec/atree.md#written-form.
pub fn to_json(graph: &Graph) -> String {
    let mut top = Map::new();
    top.insert("$dialect".into(), DIALECT.into());
    crate::structs::put_attachments(&mut top, "structs", &graph.structs);
    crate::structs::put_attachments(&mut top, "node_structs", &graph.node_structs);
    crate::structs::put_values(&mut top, &graph.values);
    if !graph.nodes.is_empty() {
        let mut seen = vec![false; graph.nodes.len()];
        top.insert("root".into(), write(graph, 0, &mut seen));
    }
    top.extend(graph.extra.clone());
    let mut text = serde_json::to_string_pretty(&Value::Object(top)).unwrap_or_default();
    text.push('\n');
    text
}

fn write(graph: &Graph, i: usize, seen: &mut [bool]) -> Value {
    seen[i] = true;
    let n = &graph.nodes[i];
    let mut o = write_node(n);
    let children: Vec<Value> = children(graph, i)
        .into_iter()
        .filter(|&c| !seen[c])
        .collect::<Vec<_>>()
        .into_iter()
        .map(|c| write(graph, c, seen))
        .collect();
    if !children.is_empty() {
        o.insert("children".into(), Value::Array(children));
    }
    o.extend(n.extra.clone());
    Value::Object(o)
}

/// Node `i`'s children, in order.
pub fn children(graph: &Graph, i: usize) -> Vec<usize> {
    let id = &graph.nodes[i].id;
    graph
        .edges
        .iter()
        .filter(|e| e.from == *id)
        .filter_map(|e| graph.node_index(&e.to))
        .collect()
}

/// Node `i`'s parent; `None` for the root.
pub fn parent(graph: &Graph, i: usize) -> Option<usize> {
    let id = &graph.nodes[i].id;
    graph
        .edges
        .iter()
        .find(|e| e.to == *id)
        .and_then(|e| graph.node_index(&e.from))
}

/// Whether node `a` is `b` or above it.
pub fn is_ancestor(graph: &Graph, a: usize, b: usize) -> bool {
    let mut at = Some(b);
    for _ in 0..=graph.nodes.len() {
        match at {
            Some(n) if n == a => return true,
            Some(n) => at = parent(graph, n),
            None => return false,
        }
    }
    false
}

/// Node `i` and everything under it, depth first.
pub fn subtree(graph: &Graph, i: usize) -> Vec<usize> {
    let mut out = vec![i];
    let mut k = 0;
    while k < out.len() {
        for c in children(graph, out[k]) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        k += 1;
    }
    out
}

/// Adds a child of `parent` at `x`, `y`; returns its index.
pub fn add_child(graph: &mut Graph, parent: usize, x: f64, y: f64) -> usize {
    let id = graph.free_id("node");
    let from = graph.nodes[parent].id.clone();
    graph.edges.push(Edge::new(from, id.clone()));
    graph.nodes.push(Node::new(id, x, y));
    graph.nodes.len() - 1
}

/// Moves node `i` (and what is under it) to be the last child of `parent`, unless that
/// would make a loop or `i` is the root.
pub fn move_under(graph: &mut Graph, i: usize, parent: usize) -> bool {
    if i == 0 || is_ancestor(graph, i, parent) {
        return false;
    }
    let id = graph.nodes[i].id.clone();
    graph.edges.retain(|e| e.to != id);
    let from = graph.nodes[parent].id.clone();
    graph.edges.push(Edge::new(from, id));
    true
}

/// Removes node `i` and everything under it; the root stays.
pub fn remove(graph: &mut Graph, i: usize) {
    if i == 0 {
        return;
    }
    let mut gone = subtree(graph, i);
    gone.sort_unstable();
    for k in gone.into_iter().rev() {
        graph.remove_node(k);
    }
}

/// Levels: the root alone is 1.
pub fn depth(graph: &Graph) -> usize {
    fn below(graph: &Graph, i: usize, guard: usize) -> usize {
        if guard == 0 {
            return 1;
        }
        1 + children(graph, i)
            .into_iter()
            .map(|c| below(graph, c, guard - 1))
            .max()
            .unwrap_or(0)
    }
    if graph.nodes.is_empty() {
        0
    } else {
        below(graph, 0, graph.nodes.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r##"{
  "$dialect": "atree/1",
  "root": {
    "id": "root",
    "label": "Root",
    "x": 0,
    "y": 0,
    "children": [
      {
        "id": "a",
        "color": "#3b82f6",
        "x": -1,
        "y": 1.5,
        "data": {
          "cost": 2
        },
        "children": [
          {
            "id": "a1",
            "x": -1,
            "y": 3
          }
        ]
      },
      {
        "id": "b",
        "x": 1,
        "y": 1.5,
        "note": "kept"
      }
    ]
  },
  "x_tool": 1
}
"##;

    #[test]
    fn trees_round_trip() {
        let tree = parse(TEXT).unwrap();
        let ids: Vec<&str> = tree.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["root", "a", "a1", "b"]);
        assert_eq!(children(&tree, 0), [1, 3]);
        assert_eq!(parent(&tree, 2), Some(1));
        assert_eq!(tree.nodes[1].color.as_deref(), Some("#3b82f6"));
        assert_eq!(depth(&tree), 3);
        assert_eq!(to_json(&tree), TEXT);
        assert!(parse(&to_json(&new())).is_ok());
        assert!(
            parse("{\"$dialect\": \"atree/1\"}")
                .unwrap_err()
                .contains("no root")
        );
    }

    #[test]
    fn moves_keep_one_root_and_no_loops() {
        let mut tree = parse(TEXT).unwrap();
        // Not under itself, nor under what is under it, nor the root anywhere.
        assert!(!move_under(&mut tree, 1, 2));
        assert!(!move_under(&mut tree, 0, 3));
        assert!(move_under(&mut tree, 1, 3));
        assert_eq!(parent(&tree, 1), Some(3));
        assert_eq!(depth(&tree), 4);
        let c = add_child(&mut tree, 2, 0.0, 5.0);
        assert_eq!(parent(&tree, c), Some(2));
        remove(&mut tree, 1);
        let ids: Vec<&str> = tree.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["root", "b"]);
        remove(&mut tree, 0);
        assert_eq!(tree.nodes.len(), 2);
    }
}
