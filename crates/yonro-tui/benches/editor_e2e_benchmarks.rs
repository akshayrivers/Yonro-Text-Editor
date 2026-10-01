use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yonro_core::{command::Edit, Buffer, BufferManager};
use yonro_tui::{
    layout::{LayoutTree, Pane, PaneContent, PaneManager, SplitDirection},
    uicomponents::{
        UIComponent, View,
    },
};
use yonro_core::highlighter::{RustSyntaxHighlighter, SyntaxHighlighter};
use yonro_tui::prelude::*;

fn simulate_e2e_editing_session() {
    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 40, width: 120 },
    };

    let mut buffer_manager = BufferManager::new();
    let mut buffer = Buffer::default();

    // 1. Initial code typing
    for line_idx in 0..50 {
        let code = format!("pub fn compute_step_{line_idx}(val: i32) -> Result<i32, Error> {{ Ok(val * 2) }}");
        for ch in code.chars() {
            buffer.insert_char(ch, Location { line_idx, grapheme_idx: buffer.grapheme_count(line_idx) });
        }
        buffer.insert_newline(Location { line_idx, grapheme_idx: buffer.grapheme_count(line_idx) });
    }

    let buffer_id = buffer_manager.add(buffer);
    let mut view = View::default();
    view.set_buffer_id(buffer_id);
    view.set_size(screen_rect);

    // 2. Navigation & search
    let buf_ref = buffer_manager.get(buffer_id).unwrap();
    let search_loc = buf_ref.search_forward("compute_step_25", Location { line_idx: 0, grapheme_idx: 0 });
    assert!(search_loc.is_some());

    // 3. Edit at target location
    let mut buf_mut = buffer_manager.get_mut(buffer_id).unwrap();
    view.handle_edit_command(Edit::Insert('X'), &mut buf_mut);

    // 4. Highlight viewport
    let mut highlighter = RustSyntaxHighlighter::default();
    for (idx, line) in buf_mut.lines().iter().enumerate().take(40) {
        highlighter.highlight(idx, line);
    }

    // 5. Layout and caret update
    let caret = view.caret_position(buf_mut);
    black_box(caret);
}

fn simulate_multi_pane_workflow() {
    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 60, width: 160 },
    };

    let mut buffer_manager = BufferManager::new();
    let b1 = buffer_manager.add(Buffer::default());
    let b2 = buffer_manager.add(Buffer::default());
    let b3 = buffer_manager.add(Buffer::default());
    let b4 = buffer_manager.add(Buffer::default());

    let mut tree = LayoutTree::new(0, screen_rect);
    let _ = tree.split_pane(0, 1, SplitDirection::Vertical, 0.5);
    let _ = tree.split_pane(0, 2, SplitDirection::Horizontal, 0.5);
    let _ = tree.split_pane(1, 3, SplitDirection::Horizontal, 0.5);

    let mut v0 = View::default(); v0.set_buffer_id(b1);
    let mut v1 = View::default(); v1.set_buffer_id(b2);
    let mut v2 = View::default(); v2.set_buffer_id(b3);
    let mut v3 = View::default(); v3.set_buffer_id(b4);

    let p0 = Pane { pane_id: 0, content: PaneContent::TextView(v0), active: true, is_floating: false, z_index: 0, is_minimized: false, rect: Rect::default() };
    let mut pane_manager = PaneManager::new(p0);
    pane_manager.create_pane(PaneContent::TextView(v1));
    pane_manager.create_pane(PaneContent::TextView(v2));
    pane_manager.create_pane(PaneContent::TextView(v3));

    tree.compute_layout(screen_rect);
    for (id, rect) in tree.collect_leaf_layouts() {
        if let Some(pane) = pane_manager.get_pane_mut(id) {
            pane.resize(rect);
        }
    }

    // Switch active panes and perform typing
    for id in [0, 1, 2, 3] {
        pane_manager.set_active_pane(id);
        if let Some(pane) = pane_manager.get_pane_mut(id) {
            if let Some(view) = pane.view_mut() {
                let buf_id = view.buffer_id();
                if let Some(buf) = buffer_manager.get_mut(buf_id) {
                    view.handle_edit_command(Edit::Insert('A'), buf);
                }
            }
        }
    }

    black_box((tree, pane_manager, buffer_manager));
}

fn bench_e2e_editor_workflows(c: &mut Criterion) {
    let mut group = c.benchmark_group("editor_e2e_workflows");

    group.bench_function("realistic_editing_session_50_lines", |b| {
        b.iter(|| {
            simulate_e2e_editing_session();
        });
    });

    group.bench_function("multi_pane_4_splits_workflow", |b| {
        b.iter(|| {
            simulate_multi_pane_workflow();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_e2e_editor_workflows);
criterion_main!(benches);
