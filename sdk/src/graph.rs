//! `.agraph.bin` directed graphs; see docs/spec/agraph.bin.md.

use crate::Result;
use crate::chunks::ChunkFile;
use crate::hash::fnv1a64;
use crate::read::Bytes;
use crate::structs::Structs;

const FILE: &str = "agraph.bin";

use crate::property::read_properties;
pub use crate::property::{Property, PropertyValue, property};

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// FNV-1a-64 of `id`.
    pub id_hash: u64,
    pub id: String,
    pub label: String,
    /// Grid units.
    pub x: f32,
    /// Grid units, down is positive.
    pub y: f32,
    /// `R G B A`; all `0` without a color.
    pub color: [u8; 4],
    pub properties: Vec<Property>,
}

impl Node {
    /// The value of property `key`.
    pub fn property(&self, key: &str) -> Option<&PropertyValue> {
        property(&self.properties, key)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Index into [`Graph::nodes`].
    pub from: u32,
    /// Index into [`Graph::nodes`].
    pub to: u32,
    pub weight: i32,
    pub label: String,
    pub properties: Vec<Property>,
}

impl Edge {
    /// The value of property `key`.
    pub fn property(&self, key: &str) -> Option<&PropertyValue> {
        property(&self.properties, key)
    }
}

/// A directed graph: nodes and the edges between them, in the file's order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Attached structs: [`Scope::Asset`](crate::structs::Scope::Asset) the graph,
    /// [`Scope::Item`](crate::structs::Scope::Item) each node and
    /// [`Scope::Edge`](crate::structs::Scope::Edge) each edge, by index.
    pub structs: Structs,
}

impl Graph {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let file = ChunkFile::open(bytes, "AGRAPH", &[1], FILE)?;
        let desc = Bytes::new(file.require("DESC", FILE)?, FILE);
        let node_count = desc.u32(0)? as usize;
        let edge_count = desc.u32(4)? as usize;
        let strings = Bytes::new(file.require("STRINGS", FILE)?, FILE);
        let props = file.chunk("PROPS").map(|p| Bytes::new(p, FILE));
        let nodes = Bytes::new(file.require("NODES", FILE)?, FILE)
            .records_n(48, node_count)?
            .map(|r| {
                Ok(Node {
                    id_hash: r.u64(0)?,
                    id: r.text_ref(8, strings)?,
                    label: r.text_ref(16, strings)?,
                    x: r.f32(24)?,
                    y: r.f32(28)?,
                    color: r.array(32)?,
                    properties: read_properties(props, strings, r.u32(36)?, r.u32(40)?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let edges = Bytes::new(file.require("EDGES", FILE)?, FILE)
            .records_n(32, edge_count)?
            .map(|r| {
                let (from, to) = (r.u32(0)?, r.u32(4)?);
                if from as usize >= node_count || to as usize >= node_count {
                    return Err(r.err(format!("edge {from} → {to} names a missing node")));
                }
                Ok(Edge {
                    from,
                    to,
                    weight: r.i32(8)?,
                    label: r.text_ref(12, strings)?,
                    properties: read_properties(props, strings, r.u32(20)?, r.u32(24)?)?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            nodes,
            edges,
            structs: Structs::from_chunks(&file, FILE)?,
        })
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

    /// The edges leaving node `index`.
    pub fn edges_from(&self, index: usize) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.from as usize == index)
    }

    /// The edges entering node `index`.
    pub fn edges_to(&self, index: usize) -> impl Iterator<Item = &Edge> {
        self.edges.iter().filter(move |e| e.to as usize == index)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::tests::file;

    /// Appends `text` to `strings`; returns its `offset len` bytes.
    pub fn text(strings: &mut Vec<u8>, text: &str) -> Vec<u8> {
        let at = strings.len() as u32;
        strings.extend_from_slice(text.as_bytes());
        [at.to_le_bytes(), (text.len() as u32).to_le_bytes()].concat()
    }

    /// A `Property` record of `key` with `type` and its 8-byte `value`.
    pub fn prop(strings: &mut Vec<u8>, key: &str, ty: u32, value: [u8; 8]) -> Vec<u8> {
        let mut out = fnv1a64(key.as_bytes()).to_le_bytes().to_vec();
        out.extend(text(strings, key));
        out.extend(ty.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(value);
        out
    }

    fn node(strings: &mut Vec<u8>, id: &str, x: f32, first: u32, count: u32) -> Vec<u8> {
        let mut out = fnv1a64(id.as_bytes()).to_le_bytes().to_vec();
        out.extend(text(strings, id));
        out.extend(text(strings, &id.to_uppercase()));
        out.extend(x.to_le_bytes());
        out.extend(2.0f32.to_le_bytes());
        out.extend([255, 0, 0, 255]);
        for v in [first, count, 0] {
            out.extend(v.to_le_bytes());
        }
        out
    }

    fn sample(drop: Option<&str>) -> Vec<u8> {
        sample_with(drop, &[])
    }

    fn sample_with(drop: Option<&str>, extra: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut nodes = node(&mut strings, "a", 1.0, 0, 1);
        nodes.extend(node(&mut strings, "b", 3.0, 1, 0));
        let mut props = prop(&mut strings, "hp", 1, (-5i64).to_le_bytes());
        let mut edges = Vec::new();
        for v in [0u32, 1] {
            edges.extend(v.to_le_bytes());
        }
        edges.extend(7i32.to_le_bytes());
        edges.extend(text(&mut strings, "go"));
        for v in [1u32, 1, 0] {
            edges.extend(v.to_le_bytes());
        }
        let (at, len) = (strings.len() as u32, 3u32);
        strings.extend_from_slice(b"key");
        let mut value = [0u8; 8];
        value[..4].copy_from_slice(&at.to_le_bytes());
        value[4..].copy_from_slice(&len.to_le_bytes());
        props.extend(prop(&mut strings, "door", 3, value));
        let mut desc = Vec::new();
        for v in [2u32, 1, 2, 0] {
            desc.extend(v.to_le_bytes());
        }
        let chunks: Vec<(&str, &[u8])> = [
            ("DESC", &desc[..]),
            ("NODES", &nodes[..]),
            ("EDGES", &edges[..]),
            ("PROPS", &props[..]),
            ("STRINGS", &strings[..]),
        ]
        .into_iter()
        .chain(extra.iter().map(|(name, payload)| (*name, &payload[..])))
        .filter(|(name, _)| Some(*name) != drop)
        .collect();
        file("AGRAPH", 1, &chunks)
    }

    #[test]
    fn decodes_nodes_edges_and_properties() {
        let graph = Graph::decode(&sample(None)).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        let a = &graph.nodes[0];
        assert_eq!((a.id.as_str(), a.label.as_str()), ("a", "A"));
        assert_eq!((a.x, a.y, a.color), (1.0, 2.0, [255, 0, 0, 255]));
        assert_eq!(a.property("hp"), Some(&PropertyValue::Int(-5)));
        assert!(graph.nodes[1].properties.is_empty());
        let edge = &graph.edges[0];
        assert_eq!((edge.from, edge.to, edge.weight), (0, 1, 7));
        assert_eq!(edge.label, "go");
        assert_eq!(
            edge.property("door"),
            Some(&PropertyValue::Text("key".into()))
        );
        assert_eq!(graph.node_index("b"), Some(1));
        assert_eq!(graph.edges_from(0).count(), 1);
        assert_eq!(graph.edges_to(0).count(), 0);
    }

    #[test]
    fn reads_attached_structs() {
        use crate::data::Value;
        use crate::structs::Scope;
        assert!(Graph::decode(&sample(None)).unwrap().structs.is_empty());
        // Scope 2: the struct on edge 1.
        let extra = crate::structs::tests::chunks(2);
        let graph = Graph::decode(&sample_with(None, &extra)).unwrap();
        assert_eq!(graph.structs.all().len(), 2);
        let meta = graph.structs.get(Scope::Asset, 0, "Meta").unwrap();
        assert_eq!(meta.get("tier"), Some(&Value::Int(3)));
        let step = graph.structs.of(Scope::Edge, 1).next().unwrap();
        assert_eq!(step.type_name, "Step");
        assert_eq!(step.get("speed").and_then(Value::as_float), Some(2.5));
        // With STRUCTS, the other struct chunks are required.
        assert!(Graph::decode(&sample_with(Some("SREFS"), &extra)).is_err());
    }

    #[test]
    fn rejects_missing_chunks() {
        assert!(Graph::decode(&sample(Some("EDGES"))).is_err());
        assert!(Graph::decode(&sample(Some("PROPS"))).is_err());
    }
}
