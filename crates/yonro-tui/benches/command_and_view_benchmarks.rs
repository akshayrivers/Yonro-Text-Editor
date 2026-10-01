use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yonro_core::{command::{Command, Edit, Move}, Buffer, BufferManager};
use yonro_tui::{
    command_dispatcher::{EditorContext, HandlerRegistry, PromptType},
    layout::{LayoutTree, Pane, PaneContent, PaneManager},
    uicomponents::{BufferBar, CommandBar, MessageBar, UIComponent, View},
};
use yonro_tui::prelude::*;

fn create_view_with_buffer(lines_count: usize) -> (View, Buffer) {
    let mut view = View::default();
    let mut buffer = Buffer::default();
    view.set_size(Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 40, width: 120 },
    });

    for i in 0..lines_count {
        for ch in format!("fn line_{i}() {{ compute_data({i}); }}\n").chars() {
            if ch == '\n' {
                view.handle_edit_command(Edit::InsertNewLine, &mut buffer);
            } else {
                view.handle_edit_command(Edit::Insert(ch), &mut buffer);
            }
        }
    }
    (view, buffer)
}

fn bench_view_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("view_operations");

    group.bench_function("view_edit_and_undo_redo_cycle", |b| {
        b.iter(|| {
            let mut view = View::default();
            let mut buffer = Buffer::default();
            view.set_size(Rect {
                position: Position { row: 1, col: 0 },
                size: Size { height: 40, width: 120 },
            });

            // Type 50 characters
            for ch in "fn test_function_with_long_identifier() { return; }".chars() {
                view.handle_edit_command(Edit::Insert(ch), &mut buffer);
            }

            // Undo all
            for _ in 0..50 {
                view.undo(&mut buffer);
            }

            // Redo all
            for _ in 0..50 {
                view.redo(&mut buffer);
            }

            black_box((view, buffer));
        });
    });

    group.bench_function("view_cursor_movement_across_document", |b| {
        let (mut view, buffer) = create_view_with_buffer(200);
        b.iter(|| {
            // Move down 50 lines
            for _ in 0..50 {
                view.handle_move_command(Move::Down, &buffer);
            }
            // Move right to end of line
            view.handle_move_command(Move::EndOfLine, &buffer);
            // Move back up
            for _ in 0..50 {
                view.handle_move_command(Move::Up, &buffer);
            }
            // Move to start of line
            view.handle_move_command(Move::StartOfLine, &buffer);

            black_box(&view);
        });
    });

    group.bench_function("view_caret_position_calculation", |b| {
        let (view, buffer) = create_view_with_buffer(50);
        b.iter(|| {
            black_box(view.caret_position(black_box(&buffer)));
        });
    });

    group.finish();
}

fn bench_command_dispatcher(c: &mut Criterion) {
    let mut group = c.benchmark_group("command_dispatcher");

    let screen_rect = Rect {
        position: Position { row: 1, col: 0 },
        size: Size { height: 40, width: 120 },
    };

    group.bench_function("dispatch_move_commands", |b| {
        let mut buffer_manager = BufferManager::new();
        let buffer_id = buffer_manager.add(Buffer::default());

        let mut initial_view = View::default();
        initial_view.set_buffer_id(buffer_id);
        initial_view.set_size(screen_rect);

        let initial_pane = Pane {
            pane_id: 0,
            content: PaneContent::TextView(initial_view),
            active: true,
            is_floating: false,
            z_index: 0,
            is_minimized: false,
            rect: screen_rect,
        };

        let mut pane_manager = PaneManager::new(initial_pane);
        let mut layout_tree = LayoutTree::new(0, screen_rect);
        let mut buffer_bar = BufferBar::default();
        let mut command_bar = CommandBar::default();
        let mut message_bar = MessageBar::default();
        let mut prompt_type = PromptType::None;
        let mut should_quit = false;
        let mut quit_times = 0;
        let mut dragging_split = None;
        let mut dragging_pane = None;
        let mut drag_offset = Position::default();
        let mut registry = HandlerRegistry::default();

        let move_down = Command::Move(Move::Down);
        let move_up = Command::Move(Move::Up);

        b.iter(|| {
            let mut ctx = EditorContext {
                prompt_type: &mut prompt_type,
                pane_manager: &mut pane_manager,
                layout_tree: &mut layout_tree,
                buffer_manager: &mut buffer_manager,
                buffer_bar: &mut buffer_bar,
                command_bar: &mut command_bar,
                message_bar: &mut message_bar,
                terminal_size: screen_rect.size,
                should_quit: &mut should_quit,
                quit_times: &mut quit_times,
                dragging_split: &mut dragging_split,
                dragging_pane: &mut dragging_pane,
                drag_offset: &mut drag_offset,
                buffer_changed: None,
            };

            let _ = registry.dispatch(&move_down, &mut ctx);
            let _ = registry.dispatch(&move_up, &mut ctx);
            black_box(&ctx.buffer_changed);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_view_operations, bench_command_dispatcher);
criterion_main!(benches);
