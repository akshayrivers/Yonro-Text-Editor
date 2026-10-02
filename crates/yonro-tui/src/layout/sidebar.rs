use std::fmt::Debug;

/// Which sidebar is being shown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarKind {
    FileExplorer,
    WordCount,
}

/// Sidebar state managed by LayoutTree
#[derive(Debug)]
pub struct Sidebar {
    pub kind: SidebarKind,
    pub visible: bool,
    pub width: usize,
    pub pane_id: Option<usize>,
}

impl Sidebar {
    pub fn new(kind: SidebarKind, width: usize) -> Self {
        Self {
            kind,
            visible: false,
            width,
            pane_id: None,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    pub fn show(&mut self) {
        self.visible = true;
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn set_pane_id(&mut self, pane_id: usize) {
        self.pane_id = Some(pane_id);
    }

    pub fn clear_pane_id(&mut self) {
        self.pane_id = None;
    }
}

impl Default for Sidebar {
    fn default() -> Self {
        Self::new(SidebarKind::FileExplorer, 30)
    }
}

/// Plugin response for sidebar operations
#[derive(Debug, Clone)]
pub enum SidebarResponse {
    ToggleSidebar { kind: SidebarKind },
    OpenSidebar { kind: SidebarKind },
    CloseSidebar { kind: SidebarKind },
}