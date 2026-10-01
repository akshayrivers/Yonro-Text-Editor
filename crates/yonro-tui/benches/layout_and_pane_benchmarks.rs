use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use yonro_tui::{
    layout::{LayoutTree, Pane, PaneContent, PaneManager, SplitDirection},
    uicomponents::View,
};
use yonro_tui::prelude::*;

fn build_nested_layout_tree(num_panes: usize, rect: Rect) -> LayoutTree {
    let mut tree = LayoutTree::new(0, rect);
    for i in 1..num_panes {
        let parent_pane = (i - 1) / 2;
        let direction = if i % 2 == 0 {
            SplitDirection::Horizontal
        } else {
            SplitDirection::Vertical
        };
        let _ = tree.split_pane(parent_pane, i, direction, 0.5);
    }
    tree
}

fn bench_layout_computation(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_tree_computation");

    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 60, width: 200 },
    };

    for &pane_count in &[2, 4, 8, 16, 32] {
        let mut tree = build_nested_layout_tree(pane_count, screen_rect);
        group.bench_with_input(
            BenchmarkId::new("compute_layout", pane_count),
            &pane_count,
            |b, _| {
                b.iter(|| {
                    tree.compute_layout(black_box(screen_rect));
                    black_box(tree.collect_leaf_layouts());
                });
            },
        );
    }

    group.finish();
}

fn bench_layout_mutations(c: &mut Criterion) {
    let mut group = c.benchmark_group("layout_tree_mutations");

    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 60, width: 200 },
    };

    group.bench_function("split_and_remove_node", |b| {
        b.iter(|| {
            let mut tree = LayoutTree::new(0, screen_rect);
            let _ = tree.split_pane(0, 1, SplitDirection::Vertical, 0.5);
            let _ = tree.split_pane(1, 2, SplitDirection::Horizontal, 0.5);
            let _ = tree.remove_node(2);
            black_box(tree);
        });
    });

    group.bench_function("find_split_hit_test", |b| {
        let mut tree = build_nested_layout_tree(8, screen_rect);
        tree.compute_layout(screen_rect);
        let mouse_pos = Position { row: 30, col: 100 };

        b.iter(|| {
            black_box(tree.find_split(black_box(mouse_pos)));
        });
    });

    group.bench_function("resize_split_handle", |b| {
        let mut tree = build_nested_layout_tree(8, screen_rect);
        tree.compute_layout(screen_rect);
        let mouse_pos = Position { row: 30, col: 120 };

        b.iter(|| {
            tree.resize_split(1, black_box(mouse_pos));
            black_box(&tree);
        });
    });

    group.finish();
}

fn bench_pane_manager(c: &mut Criterion) {
    let mut group = c.benchmark_group("pane_manager_ops");

    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 60, width: 200 },
    };

    group.bench_function("create_and_switch_panes", |b| {
        b.iter(|| {
            let initial_pane = Pane {
                pane_id: 0,
                content: PaneContent::TextView(View::default()),
                active: true,
                is_floating: false,
                z_index: 0,
                is_minimized: false,
                rect: screen_rect,
            };
            let mut manager = PaneManager::new(initial_pane);
            for _i in 1..10 {
                let id = manager.create_pane(PaneContent::TextView(View::default()));
                manager.set_active_pane(id);
            }
            black_box(manager);
        });
    });

    group.bench_function("floating_panes_z_sort_and_bring_to_front", |b| {
        let initial_pane = Pane {
            pane_id: 0,
            content: PaneContent::TextView(View::default()),
            active: true,
            is_floating: false,
            z_index: 0,
            is_minimized: false,
            rect: screen_rect,
        };
        let mut manager = PaneManager::new(initial_pane);
        let mut floating_ids = Vec::new();
        for i in 1..=8 {
            let id = manager.create_floating_pane(PaneContent::TextView(View::default()), i);
            floating_ids.push(id);
        }

        b.iter(|| {
            for &id in &floating_ids {
                manager.bring_to_front(id);
            }
            let sorted = manager.get_floating_panes_sorted();
            black_box(sorted);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_layout_computation,
    bench_layout_mutations,
    bench_pane_manager
);
criterion_main!(benches);
