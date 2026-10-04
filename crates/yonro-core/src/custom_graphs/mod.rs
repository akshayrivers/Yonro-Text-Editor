//! Hand-drawn relationship graphs (free canvas).
//!
//! The inferred graph (`crate::graph`, co-occurrence + `@mention`) stays
//! exactly one. Everything here is user-made: any number of named graphs
//! whose nodes and edges are placed by hand and never derived from text.
//! Positions are plain SVG-space numbers; the GUI owns layout, zoom, and
//! rendering, core only stores and validates.

use std::fmt;

/// Opaque id of one custom graph (stable while it lives in the store).
pub type CustomGraphId = usize;
/// Opaque id of one canvas node.
pub type CanvasNodeId = usize;
/// Opaque id of one canvas edge.
pub type CanvasEdgeId = usize;

/// Largest kept coordinate (keeps pan/zoom math finite downstream).
pub const MAX_COORD: f64 = 2000.0;

/// One hand-placed node: a free label at an (x, y) canvas position.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanvasNode {
    /// Node id, unique inside its graph.
    pub id: CanvasNodeId,
    /// Free label (never tied to a lore entity).
    pub label: String,
    /// Canvas x in SVG units.
    pub x: f64,
    /// Canvas y in SVG units.
    pub y: f64,
}

/// One hand-drawn edge between two nodes of the same graph.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CanvasEdge {
    /// Edge id, unique inside its graph.
    pub id: CanvasEdgeId,
    /// Source node id.
    pub a: CanvasNodeId,
    /// Target node id.
    pub b: CanvasNodeId,
    /// Free label (may be empty: `"rivalry"`, `""`).
    #[serde(default)]
    pub label: String,
}

/// One named free canvas.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomGraph {
    /// Graph id, unique inside the store.
    pub id: CustomGraphId,
    /// Display title.
    pub title: String,
    /// Hand-placed nodes.
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    /// Hand-drawn edges.
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
    #[serde(default)]
    next_node: CanvasNodeId,
    #[serde(default)]
    next_edge: CanvasEdgeId,
}

/// Every hand-made graph in a project (stored as `.yonro/graphs.json`).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CustomGraphStore {
    /// Graphs in creation order.
    #[serde(default)]
    pub graphs: Vec<CustomGraph>,
    #[serde(default)]
    next_id: CustomGraphId,
}

/// Validation failures (messages name the graph/node/edge and the reason).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomGraphError {
    /// Blank graph title or node label.
    EmptyLabel(String),
    /// Unknown graph id.
    UnknownGraph(CustomGraphId),
    /// Unknown node id inside a known graph.
    UnknownNode(CustomGraphId, CanvasNodeId),
    /// Unknown edge id inside a known graph.
    UnknownEdge(CustomGraphId, CanvasEdgeId),
    /// Edge needs two distinct, existing endpoints.
    BadEdge(String),
    /// Coordinate is `NaN` or infinite.
    BadCoord(String),
}

impl fmt::Display for CustomGraphError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLabel(what) => write!(formatter, "{what} cannot be empty"),
            Self::UnknownGraph(id) => write!(formatter, "unknown custom graph {id}"),
            Self::UnknownNode(graph, node) => {
                write!(formatter, "graph {graph} has no node {node}")
            }
            Self::UnknownEdge(graph, edge) => {
                write!(formatter, "graph {graph} has no edge {edge}")
            }
            Self::BadEdge(message) | Self::BadCoord(message) => write!(formatter, "{message}"),
        }
    }
}

impl std::error::Error for CustomGraphError {}

fn clean_label(raw: &str, what: &str) -> Result<String, CustomGraphError> {
    let label = raw.trim().to_string();
    if label.is_empty() {
        return Err(CustomGraphError::EmptyLabel(what.to_string()));
    }
    Ok(label)
}

fn clean_coord(raw: f64, what: &str) -> Result<f64, CustomGraphError> {
    if !raw.is_finite() {
        return Err(CustomGraphError::BadCoord(format!(
            "{what} must be a finite number"
        )));
    }
    Ok(raw.clamp(0.0, MAX_COORD))
}

impl CustomGraphStore {
    /// Empty store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graphs: Vec::new(),
            next_id: 0,
        }
    }

    /// All graphs in creation order.
    #[must_use]
    pub fn list(&self) -> &[CustomGraph] {
        &self.graphs
    }

    /// One graph by id.
    #[must_use]
    pub fn get(&self, id: CustomGraphId) -> Option<&CustomGraph> {
        self.graphs.iter().find(|graph| graph.id == id)
    }

    fn get_mut(&mut self, id: CustomGraphId) -> Result<&mut CustomGraph, CustomGraphError> {
        self.graphs
            .iter_mut()
            .find(|graph| graph.id == id)
            .ok_or(CustomGraphError::UnknownGraph(id))
    }

    /// Create a named graph.
    ///
    /// # Errors
    /// `EmptyLabel` when the title is blank.
    pub fn create(&mut self, title: &str) -> Result<CustomGraphId, CustomGraphError> {
        let title = clean_label(title, "graph title")?;
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.graphs.push(CustomGraph {
            id,
            title,
            nodes: Vec::new(),
            edges: Vec::new(),
            next_node: 0,
            next_edge: 0,
        });
        Ok(id)
    }

    /// Rename a graph.
    ///
    /// # Errors
    /// `UnknownGraph` for a bad id, `EmptyLabel` for a blank title.
    pub fn rename(&mut self, id: CustomGraphId, title: &str) -> Result<(), CustomGraphError> {
        let title = clean_label(title, "graph title")?;
        self.get_mut(id)?.title = title;
        Ok(())
    }

    /// Remove a graph and all its nodes and edges.
    ///
    /// # Errors
    /// `UnknownGraph` for a bad id.
    pub fn remove(&mut self, id: CustomGraphId) -> Result<(), CustomGraphError> {
        let at = self
            .graphs
            .iter()
            .position(|graph| graph.id == id)
            .ok_or(CustomGraphError::UnknownGraph(id))?;
        self.graphs.remove(at);
        Ok(())
    }

    /// Place a labelled node on a graph.
    ///
    /// # Errors
    /// `UnknownGraph` for a bad id, `EmptyLabel` for a blank label,
    /// `BadCoord` for non-finite coordinates.
    pub fn add_node(
        &mut self,
        graph: CustomGraphId,
        label: &str,
        x: f64,
        y: f64,
    ) -> Result<CanvasNodeId, CustomGraphError> {
        let label = clean_label(label, "node label")?;
        let x = clean_coord(x, "x")?;
        let y = clean_coord(y, "y")?;
        let target = self.get_mut(graph)?;
        let id = target.next_node;
        target.next_node = target.next_node.saturating_add(1);
        target.nodes.push(CanvasNode { id, label, x, y });
        Ok(id)
    }

    /// Move a node already on the canvas.
    ///
    /// # Errors
    /// `UnknownGraph` / `UnknownNode` for bad ids, `BadCoord` for
    /// non-finite coordinates.
    pub fn move_node(
        &mut self,
        graph: CustomGraphId,
        node: CanvasNodeId,
        x: f64,
        y: f64,
    ) -> Result<(), CustomGraphError> {
        let x = clean_coord(x, "x")?;
        let y = clean_coord(y, "y")?;
        let target = self.get_mut(graph)?;
        let slot = target
            .nodes
            .iter_mut()
            .find(|slot| slot.id == node)
            .ok_or(CustomGraphError::UnknownNode(graph, node))?;
        slot.x = x;
        slot.y = y;
        Ok(())
    }

    /// Rename a canvas node (edges follow it by id, so they keep working).
    ///
    /// # Errors
    /// `UnknownGraph` / `UnknownNode` for bad ids, `EmptyLabel` for blank.
    pub fn rename_node(
        &mut self,
        graph: CustomGraphId,
        node: CanvasNodeId,
        label: &str,
    ) -> Result<(), CustomGraphError> {
        let label = clean_label(label, "node label")?;
        let target = self.get_mut(graph)?;
        let slot = target
            .nodes
            .iter_mut()
            .find(|slot| slot.id == node)
            .ok_or(CustomGraphError::UnknownNode(graph, node))?;
        slot.label = label;
        Ok(())
    }

    /// Remove a node and every edge touching it.
    ///
    /// # Errors
    /// `UnknownGraph` / `UnknownNode` for bad ids.
    pub fn remove_node(
        &mut self,
        graph: CustomGraphId,
        node: CanvasNodeId,
    ) -> Result<(), CustomGraphError> {
        let target = self.get_mut(graph)?;
        let at = target
            .nodes
            .iter()
            .position(|slot| slot.id == node)
            .ok_or(CustomGraphError::UnknownNode(graph, node))?;
        target.nodes.remove(at);
        target.edges.retain(|edge| edge.a != node && edge.b != node);
        Ok(())
    }

    /// Draw an edge between two distinct nodes of the same graph.
    ///
    /// # Errors
    /// `UnknownGraph` for a bad graph, `BadEdge` for missing endpoints,
    /// a self-loop, or an already-drawn pair.
    pub fn add_edge(
        &mut self,
        graph: CustomGraphId,
        a: CanvasNodeId,
        b: CanvasNodeId,
        label: &str,
    ) -> Result<CanvasEdgeId, CustomGraphError> {
        let target = self.get_mut(graph)?;
        for endpoint in [a, b] {
            if target.nodes.iter().all(|slot| slot.id != endpoint) {
                return Err(CustomGraphError::BadEdge(format!(
                    "graph {graph} has no node {endpoint}"
                )));
            }
        }
        if a == b {
            return Err(CustomGraphError::BadEdge(format!(
                "graph {graph}: an edge needs two different nodes"
            )));
        }
        if target
            .edges
            .iter()
            .any(|edge| (edge.a == a && edge.b == b) || (edge.a == b && edge.b == a))
        {
            return Err(CustomGraphError::BadEdge(format!(
                "graph {graph}: those nodes are already linked"
            )));
        }
        let id = target.next_edge;
        target.next_edge = target.next_edge.saturating_add(1);
        target.edges.push(CanvasEdge {
            id,
            a,
            b,
            label: label.trim().to_string(),
        });
        Ok(id)
    }

    /// Erase an edge (nodes stay).
    ///
    /// # Errors
    /// `UnknownGraph` / `UnknownEdge` for bad ids.
    pub fn remove_edge(
        &mut self,
        graph: CustomGraphId,
        edge: CanvasEdgeId,
    ) -> Result<(), CustomGraphError> {
        let target = self.get_mut(graph)?;
        let at = target
            .edges
            .iter()
            .position(|slot| slot.id == edge)
            .ok_or(CustomGraphError::UnknownEdge(graph, edge))?;
        target.edges.remove(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_rename_remove_graph() {
        let mut store = CustomGraphStore::new();
        assert!(store.create("   ").is_err());
        let id = store.create("  Alliances  ").unwrap();
        assert_eq!(store.get(id).unwrap().title, "Alliances");
        store.rename(id, "Rivalries").unwrap();
        assert_eq!(store.get(id).unwrap().title, "Rivalries");
        assert!(store.rename(999, "Nope").is_err());
        store.remove(id).unwrap();
        assert!(store.get(id).is_none());
        assert!(store.remove(id).is_err());
    }

    #[test]
    fn nodes_move_and_take_their_edges_with_them() {
        let mut store = CustomGraphStore::new();
        let graph = store.create("Web").unwrap();
        assert!(store.add_node(graph, "  ", 1.0, 2.0).is_err());
        assert!(store.add_node(graph, "Mara", f64::NAN, 2.0).is_err());
        let mara = store.add_node(graph, "Mara", 10.0, 20.0).unwrap();
        let ion = store.add_node(graph, "Ion", 9_999.0, -5.0).unwrap();
        // Out-of-range coordinates clamp instead of failing.
        let placed = store.get(graph).unwrap();
        assert!((placed.nodes[1].x - MAX_COORD).abs() < f64::EPSILON);
        assert!(placed.nodes[1].y.abs() < f64::EPSILON);
        store.move_node(graph, mara, 30.0, 40.0).unwrap();
        assert!(store.move_node(graph, 999, 0.0, 0.0).is_err());
        let edge = store.add_edge(graph, mara, ion, "siblings").unwrap();
        assert!(store.add_edge(graph, mara, ion, "again").is_err());
        assert!(store.add_edge(graph, mara, mara, "self").is_err());
        assert!(store.add_edge(graph, mara, 999, "ghost").is_err());
        store.rename_node(graph, mara, "Mara Stone").unwrap();
        assert_eq!(store.get(graph).unwrap().nodes[0].label, "Mara Stone");
        // Removing a node drops its edges but keeps the other node.
        store.remove_node(graph, mara).unwrap();
        let kept = store.get(graph).unwrap();
        assert_eq!(kept.nodes.len(), 1);
        assert!(kept.edges.is_empty());
        assert!(store.remove_edge(graph, edge).is_err());
    }

    #[test]
    fn store_round_trips_through_json() {
        let mut store = CustomGraphStore::new();
        let graph = store.create("Web").unwrap();
        let a = store.add_node(graph, "A", 1.0, 2.0).unwrap();
        let b = store.add_node(graph, "B", 3.0, 4.0).unwrap();
        store.add_edge(graph, a, b, "").unwrap();
        let json = serde_json::to_string(&store).unwrap();
        let back: CustomGraphStore = serde_json::from_str(&json).unwrap();
        assert_eq!(store, back);
        // Missing keys read as empty, never crash (forward-compat).
        let empty: CustomGraphStore = serde_json::from_str("{}").unwrap();
        assert!(empty.list().is_empty());
    }
}
