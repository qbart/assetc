//! `.agraph` directed graphs: nodes with positions and data, directed edges with data; see
//! docs/spec/agraph.md and docs/features/graphs.md.

use std::path::Path;

use serde_json::{Map, Number, Value};

use crate::aschema::{is_identifier, json_number};
use crate::enrich::{PropValue, Properties};
use crate::structs::{self, Attachment, Values};

pub const DIALECT: &str = "agraph/1";

/// Whether `file` is a directed graph (`*.agraph`, any case).
pub fn is_graph(file: &Path) -> bool {
    file.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("agraph"))
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Graph {
    /// Structs attached to the graph (a tree), every node and every edge, and the graph's own
    /// values; see docs/features/structs.md.
    pub structs: Vec<Attachment>,
    pub node_structs: Vec<Attachment>,
    pub edge_structs: Vec<Attachment>,
    pub values: Values,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Other top-level keys, kept.
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Node {
    pub id: String,
    pub label: String,
    /// `#rrggbb`, for telling nodes apart; see docs/features/graphs.md#node-colors.
    pub color: Option<String>,
    /// The center, in grid units; Y grows down.
    pub x: f64,
    pub y: f64,
    pub data: Properties,
    pub values: Values,
    pub extra: Map<String, Value>,
}

/// Edges are told apart by their place in the graph: any number may join the same nodes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub weight: i32,
    pub data: Properties,
    pub values: Values,
    pub extra: Map<String, Value>,
}

impl Edge {
    pub fn new(from: String, to: String) -> Self {
        Self {
            from,
            to,
            label: String::new(),
            weight: 1,
            data: Properties::new(),
            values: Values::new(),
            extra: Map::new(),
        }
    }
}

impl Node {
    pub fn new(id: String, x: f64, y: f64) -> Self {
        Self {
            id,
            x,
            y,
            ..Self::default()
        }
    }

    /// What the node shows: its label, or its id without one.
    pub fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.id
        } else {
            &self.label
        }
    }
}

impl Graph {
    /// One node `start` at the origin; see docs/features/graphs.md#creating.
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                label: "Start".into(),
                ..Node::new("start".into(), 0.0, 0.0)
            }],
            ..Self::default()
        }
    }

    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// `base`, `base_2`, … not used by a node.
    pub fn free_id(&self, base: &str) -> String {
        (1..)
            .map(|n| {
                if n == 1 {
                    base.to_owned()
                } else {
                    format!("{base}_{n}")
                }
            })
            .find(|id| self.node_index(id).is_none())
            .unwrap_or_default()
    }

    /// Renames node `from` to `to`, in its edges too.
    pub fn rename_node(&mut self, from: &str, to: &str) {
        for n in &mut self.nodes {
            if n.id == from {
                n.id = to.to_owned();
            }
        }
        for e in &mut self.edges {
            if e.from == from {
                e.from = to.to_owned();
            }
            if e.to == from {
                e.to = to.to_owned();
            }
        }
    }

    /// Removes node `i` and its edges.
    pub fn remove_node(&mut self, i: usize) {
        let id = self.nodes.remove(i).id;
        self.edges.retain(|e| e.from != id && e.to != id);
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| format!("invalid JSON: {e}"))?;
        let Value::Object(mut top) = value else {
            return Err("not a JSON object".into());
        };
        match top.shift_remove("$dialect") {
            Some(Value::String(d)) if d == DIALECT => {}
            Some(Value::String(d)) => match d.strip_prefix("agraph/").map(str::parse::<u32>) {
                Some(Ok(major)) if major > 1 => {
                    return Err(format!("$dialect {d:?} is newer than {DIALECT}"));
                }
                _ => return Err(format!("unknown $dialect {d:?}")),
            },
            _ => return Err("no $dialect".into()),
        }
        let structs = structs::take_attachments(&mut top, "structs")?;
        let node_structs = structs::take_attachments(&mut top, "node_structs")?;
        let edge_structs = structs::take_attachments(&mut top, "edge_structs")?;
        let values = structs::take_values(&mut top)?;
        let array = |top: &mut Map<String, Value>, key: &str| match top.shift_remove(key) {
            Some(Value::Array(items)) => Ok(items),
            _ => Err(format!("no {key} array")),
        };
        let nodes = array(&mut top, "nodes")?
            .into_iter()
            .enumerate()
            .map(|(i, v)| parse_node(v).map_err(|e| format!("node {}: {e}", i + 1)))
            .collect::<Result<_, _>>()?;
        let edges = array(&mut top, "edges")?
            .into_iter()
            .enumerate()
            .map(|(i, v)| parse_edge(v).map_err(|e| format!("edge {}: {e}", i + 1)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            structs,
            node_structs,
            edge_structs,
            values,
            nodes,
            edges,
            extra: top,
        })
    }

    /// The written form; see docs/spec/agraph.md#written-form.
    pub fn to_json(&self) -> String {
        let mut top = Map::new();
        top.insert("$dialect".into(), DIALECT.into());
        structs::put_attachments(&mut top, "structs", &self.structs);
        structs::put_attachments(&mut top, "node_structs", &self.node_structs);
        structs::put_attachments(&mut top, "edge_structs", &self.edge_structs);
        structs::put_values(&mut top, &self.values);
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let mut o = write_node(n);
                o.extend(n.extra.clone());
                Value::Object(o)
            })
            .collect();
        top.insert("nodes".into(), Value::Array(nodes));
        let edges = self
            .edges
            .iter()
            .map(|e| {
                let mut o = Map::new();
                o.insert("from".into(), e.from.clone().into());
                o.insert("to".into(), e.to.clone().into());
                o.insert("label".into(), e.label.clone().into());
                o.insert("weight".into(), e.weight.into());
                write_data(&mut o, &e.data);
                structs::put_values(&mut o, &e.values);
                o.extend(e.extra.clone());
                Value::Object(o)
            })
            .collect();
        top.insert("edges".into(), Value::Array(edges));
        top.extend(self.extra.clone());
        let mut text = serde_json::to_string_pretty(&Value::Object(top)).unwrap_or_default();
        text.push('\n');
        text
    }

    /// The scopes of its attached structs: the graph, its nodes and its edges; see
    /// docs/features/structs.md#scopes. A tree's nodes are numbered by `places` (each node's
    /// place in its binary), and it has no edge scope.
    pub fn scopes(&self, places: Option<&[u32]>) -> Vec<structs::Scope<'_>> {
        let name = if places.is_some() { "tree" } else { "graph" };
        let mut out = vec![
            structs::Scope {
                scope: structs::ASSET,
                attachments: &self.structs,
                owners: vec![structs::Owner {
                    item: 0,
                    label: name.into(),
                    values: &self.values,
                }],
            },
            structs::Scope {
                scope: structs::ITEM,
                attachments: &self.node_structs,
                owners: self
                    .nodes
                    .iter()
                    .enumerate()
                    .map(|(i, n)| structs::Owner {
                        item: places.and_then(|p| p.get(i).copied()).unwrap_or(i as u32),
                        label: format!("node {}", n.id),
                        values: &n.values,
                    })
                    .collect(),
            },
        ];
        if places.is_none() {
            out.push(structs::Scope {
                scope: structs::EDGE,
                attachments: &self.edge_structs,
                owners: self
                    .edges
                    .iter()
                    .enumerate()
                    .map(|(k, e)| structs::Owner {
                        item: k as u32,
                        label: format!("edge {} -> {}", e.from, e.to),
                        values: &e.values,
                    })
                    .collect(),
            });
        }
        out
    }

    /// Ids and edges that break the rules of docs/spec/agraph.md; loops and repeated edges
    /// are fine.
    pub fn check(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if !is_identifier(&n.id) {
                out.push(format!("node {:?}: the id is not an identifier", n.id));
            }
            if self.nodes[..i].iter().any(|o| o.id == n.id) {
                out.push(format!("node {:?}: the id is used twice", n.id));
            }
        }
        for e in &self.edges {
            let name = format!("edge {} → {}", e.from, e.to);
            for end in [&e.from, &e.to] {
                if self.node_index(end).is_none() {
                    out.push(format!("{name}: no node {end:?}"));
                }
            }
        }
        out
    }
}

fn text(o: &mut Map<String, Value>, key: &str, required: bool) -> Result<String, String> {
    match o.shift_remove(key) {
        Some(Value::String(s)) => Ok(s),
        None if !required => Ok(String::new()),
        None => Err(format!("no {key}")),
        Some(_) => Err(format!("{key} is not a string")),
    }
}

fn number(o: &mut Map<String, Value>, key: &str) -> Result<f64, String> {
    o.shift_remove(key)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| format!("{key} is not a number"))
}

fn parse_data(o: &mut Map<String, Value>) -> Result<Properties, String> {
    let Some(data) = o.shift_remove("data") else {
        return Ok(Properties::new());
    };
    let Value::Object(data) = data else {
        return Err("data is not an object".into());
    };
    data.into_iter()
        .map(|(key, value)| {
            let value = match value {
                Value::Bool(b) => PropValue::Bool(b),
                Value::Number(n) => match n.as_i64() {
                    Some(i) => PropValue::Int(i),
                    None => PropValue::Float(n.as_f64().unwrap_or(0.0)),
                },
                Value::String(s) => PropValue::Text(s),
                _ => return Err(format!("data {key:?} is not a bool, number or string")),
            };
            Ok((key, value))
        })
        .collect()
}

fn write_data(o: &mut Map<String, Value>, data: &Properties) {
    if data.is_empty() {
        return;
    }
    let data: Map<String, Value> = data
        .iter()
        .map(|(k, v)| {
            let v = match v {
                PropValue::Bool(b) => Value::Bool(*b),
                PropValue::Int(i) => Value::from(*i),
                PropValue::Float(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
                PropValue::Text(t) => Value::String(t.clone()),
            };
            (k.clone(), v)
        })
        .collect();
    o.insert("data".into(), Value::Object(data));
}

/// A node's keys in the written form; other keys are added after.
pub(crate) fn write_node(n: &Node) -> Map<String, Value> {
    let mut o = Map::new();
    o.insert("id".into(), n.id.clone().into());
    if !n.label.is_empty() {
        o.insert("label".into(), n.label.clone().into());
    }
    if let Some(color) = &n.color {
        o.insert("color".into(), color.clone().into());
    }
    o.insert("x".into(), json_number(n.x));
    o.insert("y".into(), json_number(n.y));
    write_data(&mut o, &n.data);
    structs::put_values(&mut o, &n.values);
    o
}

pub(crate) fn parse_node(value: Value) -> Result<Node, String> {
    let Value::Object(mut o) = value else {
        return Err("not an object".into());
    };
    let color = match o.shift_remove("color") {
        None => None,
        Some(Value::String(c)) if crate::amod::locator_color(&c).is_some() => Some(c),
        Some(_) => return Err("color is not #rrggbb".into()),
    };
    Ok(Node {
        id: text(&mut o, "id", true)?,
        label: text(&mut o, "label", false)?,
        color,
        x: number(&mut o, "x")?,
        y: number(&mut o, "y")?,
        data: parse_data(&mut o)?,
        values: structs::take_values(&mut o)?,
        extra: o,
    })
}

fn parse_edge(value: Value) -> Result<Edge, String> {
    let Value::Object(mut o) = value else {
        return Err("not an object".into());
    };
    let weight = match o.shift_remove("weight") {
        None => 1,
        Some(v) => v
            .as_i64()
            .and_then(|w| i32::try_from(w).ok())
            .ok_or("weight is not a 32-bit integer")?,
    };
    Ok(Edge {
        from: text(&mut o, "from", true)?,
        to: text(&mut o, "to", true)?,
        label: text(&mut o, "label", false)?,
        weight,
        data: parse_data(&mut o)?,
        values: structs::take_values(&mut o)?,
        extra: o,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structs_round_trip() {
        let text = r##"{
  "$dialect": "agraph/1",
  "structs": [
    {
      "schema": "game.aschema",
      "type": "Quest"
    }
  ],
  "node_structs": [
    {
      "schema": "game.aschema",
      "type": "Step"
    }
  ],
  "values": {
    "Quest": {
      "title": "Fox Hunt"
    }
  },
  "nodes": [
    {
      "id": "start",
      "x": 0,
      "y": 0,
      "values": {
        "Step": {
          "cost": 2
        }
      }
    }
  ],
  "edges": []
}
"##;
        let graph = Graph::parse(text).unwrap();
        assert_eq!(graph.structs[0].ty, "Quest");
        assert_eq!(graph.node_structs[0].schema, "game.aschema");
        assert!(graph.edge_structs.is_empty());
        assert_eq!(graph.nodes[0].values["Step"]["cost"], 2);
        assert_eq!(graph.to_json(), text);
        let scopes = graph.scopes(None);
        assert_eq!(scopes.len(), 3);
        assert_eq!(scopes[1].owners[0].label, "node start");
        assert!(
            Graph::parse(&text.replace(
                "\"values\": {\n        \"Step\"",
                "\"values\": {\n        \"Step\": 1, \"X\""
            ))
            .is_err()
        );
    }

    #[test]
    fn graphs_round_trip() {
        let text = r##"{
  "$dialect": "agraph/1",
  "nodes": [
    {
      "id": "start",
      "label": "Start",
      "x": 0,
      "y": 0
    },
    {
      "id": "dash",
      "color": "#ef4444",
      "x": 2.5,
      "y": -1.25,
      "data": {
        "cost": 2,
        "name": "Dash",
        "speed": 1.0,
        "unlocked": false
      },
      "note": "red"
    }
  ],
  "edges": [
    {
      "from": "start",
      "to": "dash",
      "label": "unlock",
      "weight": -3,
      "data": {
        "level": 3
      }
    }
  ],
  "x_tool": 1
}
"##;
        let graph = Graph::parse(text).unwrap();
        assert_eq!(graph.nodes[1].data["speed"], PropValue::Float(1.0));
        assert_eq!(graph.nodes[1].data["cost"], PropValue::Int(2));
        assert_eq!(graph.nodes[1].color.as_deref(), Some("#ef4444"));
        assert_eq!(graph.check(), Vec::<String>::new());
        assert_eq!(graph.to_json(), text);
        assert_eq!(Graph::parse(&Graph::new().to_json()).unwrap(), Graph::new());
    }

    #[test]
    fn edits_keep_edges_whole() {
        let mut graph = Graph::new();
        graph.nodes.push(Node::new(graph.free_id("node"), 1.0, 0.0));
        graph.nodes.push(Node::new(graph.free_id("node"), 2.0, 0.0));
        assert_eq!(graph.nodes[2].id, "node_2");
        graph.edges.push(Edge::new("start".into(), "node".into()));
        graph.edges.push(Edge::new("start".into(), "node".into()));
        graph.edges.push(Edge::new("node".into(), "node".into()));
        assert_eq!(graph.check(), Vec::<String>::new());
        graph.rename_node("node", "dash");
        assert_eq!(graph.edges[0].to, "dash");
        assert_eq!(graph.edges[2].from, "dash");
        graph.remove_node(1);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn problems_and_unreadable_files() {
        let mut graph = Graph::new();
        graph.nodes.push(Node::new("start".into(), 1.0, 0.0));
        graph.nodes.push(Node::new("9lives".into(), 1.0, 0.0));
        graph
            .edges
            .push(Edge::new("start".into(), "nowhere".into()));
        let problems = graph.check().join("\n");
        assert!(problems.contains("used twice"));
        assert!(problems.contains("not an identifier"));
        assert!(problems.contains("no node \"nowhere\""));
        assert!(Graph::parse("{}").unwrap_err().contains("no $dialect"));
        let newer = r#"{"$dialect": "agraph/2", "nodes": [], "edges": []}"#;
        assert!(Graph::parse(newer).unwrap_err().contains("newer"));
        let bad = r#"{"$dialect": "agraph/1", "nodes": [{"id": "a"}], "edges": []}"#;
        assert!(Graph::parse(bad).unwrap_err().contains("node 1: x"));
        let heavy = r#"{"$dialect": "agraph/1", "nodes": [], "edges": [{"from": "a", "to": "b", "weight": 3000000000}]}"#;
        assert!(Graph::parse(heavy).unwrap_err().contains("edge 1: weight"));
        let light = r#"{"$dialect": "agraph/1", "nodes": [], "edges": [{"from": "a", "to": "b"}]}"#;
        let edge = &Graph::parse(light).unwrap().edges[0];
        assert_eq!((edge.label.as_str(), edge.weight), ("", 1));
    }
}
