// Document tabs (VS Code-style): each opened file gets its own root pane —
// full editor area, no splitting. Only the active tab's layout is installed
// in `LayoutTree`; the rest are stashed here with their split layouts intact.

use super::layouttree::LayoutNode;

/// One open document tab: a stashed layout subtree plus the pane to focus.
#[derive(Debug, Clone)]
pub struct DocTab {
    /// Layout subtree for this tab (installed into the tree while active).
    pub root: LayoutNode,
    /// Pane to focus when this tab becomes active.
    pub active_pane: usize,
}

impl DocTab {
    pub fn new(root: LayoutNode, active_pane: usize) -> Self {
        Self { root, active_pane }
    }

    /// Every pane id referenced by this tab's subtree.
    pub fn pane_ids(&self) -> Vec<usize> {
        let mut out = Vec::new();
        collect_pane_ids(&self.root, &mut out);
        out
    }
}

fn collect_pane_ids(node: &LayoutNode, out: &mut Vec<usize>) {
    match node {
        LayoutNode::Leaf { pane_id, .. } => out.push(*pane_id),
        LayoutNode::Split { first, second, .. } => {
            collect_pane_ids(first, out);
            collect_pane_ids(second, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::layouttree::LayoutTree;
    use super::DocTab;
    use crate::prelude::*;

    fn leaf(id: usize) -> super::super::layouttree::LayoutNode {
        super::super::layouttree::LayoutNode::Leaf {
            pane_id: id,
            rect: Rect::default(),
        }
    }

    fn big_rect() -> Rect {
        Rect {
            position: Position { row: 1, col: 0 },
            size: Size {
                height: 20,
                width: 80,
            },
        }
    }

    #[test]
    fn tab_pane_ids_collects_split_leaves() {
        let mut tree = LayoutTree::new(0, big_rect());
        tree.split_pane(
            0,
            1,
            super::super::layouttree::SplitDirection::Vertical,
            0.5,
        )
        .unwrap();
        // Rebuild the same shape for the tab via a fresh tree root clone.
        let tab = DocTab::new(tree.clone_root(), 1);
        let mut ids = tab.pane_ids();
        ids.sort_unstable();
        assert_eq!(ids, vec![0, 1]);
    }

    #[test]
    fn remove_from_root_collapses_split_and_empties_single_leaf() {
        let mut tree = LayoutTree::new(0, big_rect());
        tree.split_pane(
            0,
            1,
            super::super::layouttree::SplitDirection::Vertical,
            0.5,
        )
        .unwrap();
        let root = tree.clone_root();
        // Removing one side collapses to the sibling leaf.
        let collapsed = LayoutTree::remove_from_root(root, 0).expect("sibling survives");
        let tab = DocTab::new(collapsed, 1);
        assert_eq!(tab.pane_ids(), vec![1]);
        // Removing the last leaf empties the root.
        assert!(LayoutTree::remove_from_root(leaf(7), 7).is_none());
        // Unknown id leaves the root intact.
        assert!(LayoutTree::remove_from_root(leaf(7), 99).is_some());
    }
}
