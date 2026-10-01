/*
Phase II - building the pane abstraction:
there are multiple apporaches by which we can approach this problem
- see usually editors have -> 1. LayoutTree 2.Pane Manager 3.Renderer 4.Buffers
- Till now we already had a basic renderer and buffer which can be further extended

Now Layouts can also be done in two ways ->
    1. Flat Layout [Vec<Pane>] which is very painful in ediotrs and can become messy when i will introduce the dynamic sizes and resizing propagation becomes messy
    2. Layout Tree (most of the editors use this(vim,vscode..)) and as our splitting is mostly vertical or horizonatal that is why Binary
    Trees are the go to choice here

    File structures in /editor/layout :
    layouttree.rs
        - recursive split tree
        - layout subdivision
        - structural representation only

    pane.rs
        - Geometry abstraction and dispather

    panemanager.rs
        - pane lookup and lifecycle
        - active pane tracking

    mod.rs
        - public layout/pane APIs
        - integration with editor render loop

*/

pub mod layouttree;
pub mod pane;
pub mod panemanager;

pub use layouttree::{LayoutNode, LayoutTree, SplitDirection, SplitHandle};
pub use pane::{Pane, PaneContent};
pub use panemanager::PaneManager;
