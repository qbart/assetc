//! `.atree.bin` trees, nodes breadth first; see docs/spec/atree.bin.md.

use std::ops::Range;

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::property::{Property, PropertyValue, property, read_properties};
use crate::read::Bytes;
use crate::structs::Structs;

const FILE: &str = "atree.bin";

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// FNV-1a-64 of `id`.
    pub id_hash: u64,
    pub id: String,
    pub label: String,
    pub x: f32,
    pub y: f32,
    /// `R G B A`; all `0` without a color.
    pub color: [u8; 4],
    /// Index into [`Tree::nodes`]; `None` for the root.
    pub parent: Option<u32>,
    /// Index of its first child; its children follow one another.
    pub first_child: u32,
    pub child_count: u32,
    /// The root is `0`.
    pub depth: u32,
    pub properties: Vec<Property>,
}

impl Node {
    /// Indices of its children in [`Tree::nodes`].
    pub fn children(&self) -> Range<usize> {
        let first = self.first_child as usize;
        first..first + self.child_count as usize
    }

    /// The value of property `key`.
    pub fn property(&self, key: &str) -> Option<&PropertyValue> {
        property(&self.properties, key)
    }
}

/// A tree: the root first, then level by level, each node's children in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    /// Levels; the root alone is `1`.
    pub depth: u32,
    /// Attached structs: [`Scope::Asset`](crate::structs::Scope::Asset) the tree,
    /// [`Scope::Item`](crate::structs::Scope::Item) each node, by index into `nodes`.
    pub structs: Structs,
}

impl Tree {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "ATREE", &[1], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let node_count = desc.u32(0)? as usize;
        let depth = desc.u32(8)?;
        let strings = Bytes::new(file.require("STRINGS", FILE)?, FILE);
        let props = file.chunk("PROPS").map(|p| Bytes::new(p, FILE));
        let nodes = Bytes::new(file.require("NODES", FILE)?, FILE)
            .records_n(64, node_count)?
            .map(|r| {
                let parent = match r.i32(36)? {
                    -1 => None,
                    p if p >= 0 && (p as usize) < node_count => Some(p as u32),
                    p => return Err(r.err(format!("parent {p} is not a node"))),
                };
                let (first_child, child_count) = (r.u32(40)?, r.u32(44)?);
                if first_child as u64 + child_count as u64 > node_count as u64 {
                    return Err(r.err("children run past the nodes"));
                }
                Ok(Node {
                    id_hash: r.u64(0)?,
                    id: r.text_ref(8, strings)?,
                    label: r.text_ref(16, strings)?,
                    x: r.f32(24)?,
                    y: r.f32(28)?,
                    color: r.array(32)?,
                    parent,
                    first_child,
                    child_count,
                    depth: r.u32(56)?,
                    properties: read_properties(props, strings, r.u32(48)?, r.u32(52)?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            nodes,
            depth,
            structs: Structs::from_chunks(&file, FILE)?,
        })
    }

    /// The root, unless the tree is empty.
    pub fn root(&self) -> Option<&Node> {
        self.nodes.first()
    }

    /// The children of node `index`.
    pub fn children(&self, index: usize) -> &[Node] {
        self.nodes
            .get(index)
            .and_then(|n| self.nodes.get(n.children()))
            .unwrap_or(&[])
    }

    /// The parent of node `index`.
    pub fn parent(&self, index: usize) -> Option<&Node> {
        let parent = self.nodes.get(index)?.parent?;
        self.nodes.get(parent as usize)
    }

    /// The index of the node with `id`.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        let hash = fnv1a64(id.as_bytes());
        self.nodes
            .iter()
            .position(|n| n.id_hash == hash && n.id == id)
    }

    /// The node with `id`.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.node_index(id).map(|i| &self.nodes[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::tests::file;
    use crate::graph::tests::{prop, text};

    fn node(strings: &mut Vec<u8>, id: &str, parent: i32, children: [u32; 2]) -> Vec<u8> {
        let mut out = fnv1a64(id.as_bytes()).to_le_bytes().to_vec();
        out.extend(text(strings, id));
        out.extend(text(strings, ""));
        out.extend(0.5f32.to_le_bytes());
        out.extend(1.5f32.to_le_bytes());
        out.extend([0; 4]);
        out.extend(parent.to_le_bytes());
        let (props, depth) = if parent < 0 { (0, 0) } else { (1, 1) };
        for v in [children[0], children[1], 0, props, depth, 0] {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn sample(nodes_len: Option<usize>) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut nodes = node(&mut strings, "root", -1, [1, 2]);
        nodes.extend(node(&mut strings, "left", 0, [0, 0]));
        nodes.extend(node(&mut strings, "right", 0, [0, 0]));
        let props = prop(&mut strings, "chance", 2, 0.25f64.to_le_bytes());
        let mut desc = Vec::new();
        for v in [3u32, 1, 2, 0] {
            desc.extend(v.to_le_bytes());
        }
        let nodes = &nodes[..nodes_len.unwrap_or(nodes.len())];
        file(
            "ATREE",
            1,
            &[
                ("DESC", &desc),
                ("NODES", nodes),
                ("PROPS", &props),
                ("STRINGS", &strings),
            ],
        )
    }

    #[test]
    fn decodes_parents_and_children() {
        let tree = Tree::decode(&sample(None)).unwrap();
        assert_eq!((tree.nodes.len(), tree.depth), (3, 2));
        let root = tree.root().unwrap();
        assert_eq!((root.id.as_str(), root.parent), ("root", None));
        let children: Vec<&str> = tree.children(0).iter().map(|n| n.id.as_str()).collect();
        assert_eq!(children, ["left", "right"]);
        assert!(tree.children(1).is_empty());
        assert_eq!(tree.parent(2).map(|n| n.id.as_str()), Some("root"));
        let right = tree.node("right").unwrap();
        assert_eq!((right.x, right.y, right.depth), (0.5, 1.5, 1));
        assert_eq!(right.property("chance"), Some(&PropertyValue::Float(0.25)));
    }

    #[test]
    fn rejects_truncated_nodes() {
        assert!(Tree::decode(&sample(Some(64 * 2 + 10))).is_err());
    }
}
