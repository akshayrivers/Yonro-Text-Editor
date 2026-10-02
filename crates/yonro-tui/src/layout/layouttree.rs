use crate::prelude::*;
use crate::layout::sidebar::Sidebar;
use std::io::Error;

const MIN_PANE_SIZE: usize = 4; // minimum size to split is 2 * MIN_PANE_SIZE

pub enum LayoutNode {
    Split {
        split_id: usize,
        direction: SplitDirection,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
        rect: Rect,
    },
    Leaf {
        pane_id: usize,
        rect: Rect,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}
/// Result of removing one pane from a layout subtree.
enum RemovalResult {
    /// Target was in this subtree; carries the rebuilt subtree.
    Survived(LayoutNode),
    /// This node itself was the target leaf; the parent must use the sibling.
    Deleted,
}
pub struct SplitHandle {
    pub id: usize,
    pub direction: SplitDirection,
    pub rect: Rect,
}
pub struct LayoutTree {
    root: LayoutNode,
    next_split_id: usize,
    pub sidebar: Sidebar,
}

impl Default for LayoutTree {
    fn default() -> Self {
        Self::new(0, Rect::default())
    }
}

impl LayoutTree {
    // construction
    pub fn new(initial_pane_id: usize, rect: Rect) -> Self {
        Self {
            root: LayoutNode::Leaf {
                pane_id: initial_pane_id,
                rect,
            },
            next_split_id: 1,
            sidebar: Sidebar::default(),
        }
    }

    // layout computation
    pub fn compute_layout(&mut self, rect: Rect) {
        let available_rect = if self.sidebar.visible {
            Rect {
                position: Position {
                    row: rect.position.row,
                    col: rect.position.col,
                },
                size: Size {
                    width: rect.size.width.saturating_sub(self.sidebar.width),
                    height: rect.size.height,
                },
            }
        } else {
            rect
        };
        Self::compute_node_layout(&mut self.root, available_rect);
    }

    fn compute_node_layout(node: &mut LayoutNode, rect: Rect) {
        match node {
            LayoutNode::Leaf {
                rect: node_rect, ..
            } => {
                *node_rect = rect;
            }

            LayoutNode::Split {
                split_id: _,
                direction,
                ratio,
                first,
                second,
                rect: node_rect,
            } => {
                *node_rect = rect;

                match direction {
                    SplitDirection::Vertical => {
                        let max_width = rect.size.width.saturating_sub(1);
                        let first_width = if max_width >= 1 {
                            ((rect.size.width as f32 * *ratio) as usize).clamp(1, max_width)
                        } else {
                            rect.size.width
                        };
                        let second_width = rect.size.width.saturating_sub(first_width);

                        let first_rect = Rect {
                            position: rect.position,
                            size: Size {
                                width: first_width,
                                height: rect.size.height,
                            },
                        };

                        let second_rect = Rect {
                            position: Position {
                                row: rect.position.row,
                                col: rect.position.col + first_width,
                            },
                            size: Size {
                                width: second_width,
                                height: rect.size.height,
                            },
                        };

                        Self::compute_node_layout(first, first_rect);
                        Self::compute_node_layout(second, second_rect);
                    }

                    SplitDirection::Horizontal => {
                        let max_height = rect.size.height.saturating_sub(1);
                        let first_height = if max_height >= 1 {
                            ((rect.size.height as f32 * *ratio) as usize).clamp(1, max_height)
                        } else {
                            rect.size.height
                        };
                        let second_height = rect.size.height.saturating_sub(first_height);

                        let first_rect = Rect {
                            position: rect.position,
                            size: Size {
                                width: rect.size.width,
                                height: first_height,
                            },
                        };

                        let second_rect = Rect {
                            position: Position {
                                row: rect.position.row + first_height,
                                col: rect.position.col,
                            },
                            size: Size {
                                width: rect.size.width,
                                height: second_height,
                            },
                        };

                        Self::compute_node_layout(first, first_rect);
                        Self::compute_node_layout(second, second_rect);
                    }
                }
            }
        }
    }

    // mutation
    pub fn split_pane(
        &mut self,
        target_pane_id: usize,
        new_pane_id: usize,
        direction: SplitDirection,
        ratio: f32,
    ) -> Result<(), Error> {
        let split_id = self.next_split_id;
        self.next_split_id += 1;
        Self::split_node(
            &mut self.root,
            target_pane_id,
            new_pane_id,
            direction,
            ratio,
            split_id,
        )
    }

    fn split_node(
        node: &mut LayoutNode,
        target_pane_id: usize,
        new_pane_id: usize,
        direction: SplitDirection,
        ratio: f32,
        split_id: usize,
    ) -> Result<(), Error> {
        match node {
            LayoutNode::Leaf { pane_id, rect } => {
                if *pane_id == target_pane_id {
                    // Check for minimum size
                    let min_needed = 2 * MIN_PANE_SIZE;
                    let current_size = match direction {
                        SplitDirection::Vertical => rect.size.width,
                        SplitDirection::Horizontal => rect.size.height,
                    };

                    if current_size < min_needed {
                        return Err(Error::other("Pane too small to split"));
                    }

                    let old_rect = *rect;

                    let old_leaf = LayoutNode::Leaf {
                        pane_id: *pane_id,
                        rect: old_rect,
                    };

                    let new_leaf = LayoutNode::Leaf {
                        pane_id: new_pane_id,
                        rect: old_rect,
                    };

                    *node = LayoutNode::Split {
                        split_id: split_id,
                        direction,
                        ratio,
                        first: Box::new(old_leaf),
                        second: Box::new(new_leaf),
                        rect: old_rect,
                    };

                    Ok(())
                } else {
                    Err(Error::other("Pane not found"))
                }
            }

            LayoutNode::Split { first, second, .. } => {
                if Self::split_node(
                    first,
                    target_pane_id,
                    new_pane_id,
                    direction,
                    ratio,
                    split_id,
                )
                .is_ok()
                {
                    return Ok(());
                }

                Self::split_node(
                    second,
                    target_pane_id,
                    new_pane_id,
                    direction,
                    ratio,
                    split_id,
                )
            }
        }
    }
    // when the child node is removed we need to adjust the tree accordingly too
    pub fn remove_node(&mut self, id: usize) -> Result<(), Error> {
        // if there is only one pane left we do not allow closing
        if let LayoutNode::Leaf { pane_id, .. } = self.root {
            if pane_id == id {
                return Err(Error::other("Cannot remove the last remaining pane"));
            }
        }

        let old_root = std::mem::replace(
            &mut self.root,
            LayoutNode::Leaf {
                pane_id: 0,
                rect: Rect::default(),
            },
        );
        match self.remove_node_recursive(old_root, id) {
            RemovalResult::Survived(new_root) => {
                self.root = new_root;
                Ok(())
            }
            // Target was the root leaf itself (guarded above, but stay total).
            RemovalResult::Deleted => Err(Error::other("Unexpected deletion of root")),
        }
    }

    /// Type-safe removal signal — replaces the old `Err(Error::other("DELETED"))`
    /// string-comparison hack (`PLAN.md Phase 1.6`). `io::Error` is for I/O
    /// failures, never for control flow.
    fn remove_node_recursive(&self, node: LayoutNode, target_id: usize) -> RemovalResult {
        match node {
            LayoutNode::Leaf { pane_id, rect } => {
                if pane_id == target_id {
                    return RemovalResult::Deleted;
                }
                RemovalResult::Survived(LayoutNode::Leaf { pane_id, rect })
            }
            LayoutNode::Split {
                split_id,
                direction,
                ratio,
                first,
                second,
                rect,
            } => {
                match self.remove_node_recursive(*first, target_id) {
                    // First child was (or contained) the target: sibling takes over.
                    RemovalResult::Deleted => RemovalResult::Survived(*second),
                    RemovalResult::Survived(new_first) => {
                        match self.remove_node_recursive(*second, target_id) {
                            RemovalResult::Deleted => {
                                RemovalResult::Survived(new_first)
                            }
                            RemovalResult::Survived(new_second) => {
                                // neither was the target, so we rebuild the split
                                RemovalResult::Survived(LayoutNode::Split {
                                    split_id,
                                    direction,
                                    ratio,
                                    first: Box::new(new_first),
                                    second: Box::new(new_second),
                                    rect,
                                })
                            }
                        }
                    }
                }
            }
        }
    }
    pub fn find_split(&self, mouse: Position) -> Option<SplitHandle> {
        Self::find_split_at(&self.root, mouse)
    }

    pub fn find_split_at(node: &LayoutNode, mouse: Position) -> Option<SplitHandle> {
        const TOLERANCE: usize = 2; // 2 character tolerance zone

        match node {
            LayoutNode::Leaf { .. } => None,
            LayoutNode::Split {
                split_id,
                direction,
                ratio,
                first,
                second,
                rect,
            } => {
                match direction {
                    SplitDirection::Vertical => {
                        let first_width = ((rect.size.width as f32 * *ratio) as usize)
                            .clamp(1, rect.size.width.saturating_sub(1));
                        let divider_col = rect.position.col + first_width;

                        let inside_vertical_span = mouse.row >= rect.position.row
                            && mouse.row < rect.position.row + rect.size.height;

                        // Add tolerance zone
                        let mouse_over_divider = mouse.col >= divider_col.saturating_sub(TOLERANCE)
                            && mouse.col <= divider_col.saturating_add(TOLERANCE);

                        if mouse_over_divider && inside_vertical_span {
                            return Some(SplitHandle {
                                id: *split_id,
                                direction: *direction,
                                rect: *rect,
                            });
                        }
                    }
                    SplitDirection::Horizontal => {
                        let first_height = ((rect.size.height as f32 * *ratio) as usize)
                            .clamp(1, rect.size.height.saturating_sub(1));
                        let divider_row = rect.position.row + first_height;

                        let inside_horizontal_span = mouse.col >= rect.position.col
                            && mouse.col < rect.position.col + rect.size.width;

                        let mouse_over_divider = mouse.row >= divider_row.saturating_sub(TOLERANCE)
                            && mouse.row <= divider_row.saturating_add(TOLERANCE);

                        if mouse_over_divider && inside_horizontal_span {
                            return Some(SplitHandle {
                                id: *split_id,
                                direction: *direction,
                                rect: *rect,
                            });
                        }
                    }
                }
                Self::find_split_at(first, mouse).or_else(|| Self::find_split_at(second, mouse))
            }
        }
    }
    pub fn resize_split(&mut self, split_id: usize, mouse: Position) {
        Self::resize_node(&mut self.root, split_id, mouse);
    }

    fn resize_node(node: &mut LayoutNode, split_id: usize, mouse: Position) {
        match node {
            LayoutNode::Leaf { .. } => {}
            LayoutNode::Split {
                split_id: id,
                direction,
                ratio,
                first,
                second,
                rect,
            } => {
                if *id == split_id {
                    match direction {
                        SplitDirection::Vertical => {
                            let min_width = MIN_PANE_SIZE;
                            let max_width = rect.size.width.saturating_sub(MIN_PANE_SIZE);
                            if max_width >= min_width && rect.size.width > 0 {
                                let local_col = mouse.col.saturating_sub(rect.position.col).clamp(min_width, max_width);
                                let new_ratio = local_col as f32 / rect.size.width as f32;

                                #[cfg(debug_assertions)]
                                println!("Resize: mouse.col={}, rect.pos.col={}, local_col={}, rect.width={}, old_ratio={}, new_ratio={}", 
                                mouse.col, rect.position.col, local_col, rect.size.width, ratio, new_ratio);

                                *ratio = new_ratio;
                            }
                        }
                        SplitDirection::Horizontal => {
                            let min_height = MIN_PANE_SIZE;
                            let max_height = rect.size.height.saturating_sub(MIN_PANE_SIZE);
                            if max_height >= min_height && rect.size.height > 0 {
                                let local_row = mouse.row.saturating_sub(rect.position.row).clamp(min_height, max_height);
                                let new_ratio = local_row as f32 / rect.size.height as f32;

                                #[cfg(debug_assertions)]
                                println!("Resize: mouse.row={}, rect.pos.row={}, local_row={}, rect.height={}, old_ratio={}, new_ratio={}", 
                                mouse.row, rect.position.row, local_row, rect.size.height, ratio, new_ratio);

                                *ratio = new_ratio;
                            }
                        }
                    }
                } else {
                    Self::resize_node(first, split_id, mouse);
                    Self::resize_node(second, split_id, mouse);
                }
            }
        }
    }

    pub fn collect_leaf_layouts(&self) -> Vec<(usize, Rect)> {
        let mut layouts = Vec::new();
        Self::collect_layouts(&self.root, &mut layouts);
        layouts
    }

    fn collect_layouts(node: &LayoutNode, layouts: &mut Vec<(usize, Rect)>) {
        match node {
            LayoutNode::Leaf { pane_id, rect } => {
                layouts.push((*pane_id, *rect));
            }

            LayoutNode::Split { first, second, .. } => {
                Self::collect_layouts(first, layouts);
                Self::collect_layouts(second, layouts);
            }
        }
    }
}
