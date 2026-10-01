use std::io::Write;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use yonro_core::{
    annotatedstring::AnnotatedString, annotationtype::AnnotationType, buffers::{Buffer, BufferManager},
    command::{Command, Edit, Move}, events::EditorEvent,
    highlighter::{MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SyntaxHighlighter}, Line,
};
use yonro_tui::{
    command_dispatcher::{EditorContext, HandlerRegistry, PromptType},
    layout::{LayoutTree, Pane, PaneContent, PaneManager, SplitDirection},
    plugins::{BufferSnapshot, PluginMessage, PluginRuntime},
    plugins::builtin::FileExplorerPlugin,
    uicomponents::{BufferBar, CommandBar, MessageBar, UIComponent, View},
};
use yonro_tui::prelude::*;

#[allow(dead_code)]
struct BenchResult {
    category: &'static str,
    name: &'static str,
    iterations: usize,
    total_time: Duration,
    min_time: Duration,
    mean_time: Duration,
    median_time: Duration,
    p95_time: Duration,
    p99_time: Duration,
    ops_per_sec: f64,
    throughput_mb_s: Option<f64>,
}

fn run_benchmark<F>(category: &'static str, name: &'static str, bytes_per_op: Option<usize>, target_duration: Duration, mut f: F) -> BenchResult
where
    F: FnMut(),
{
    // Warmup
    let warmup_start = Instant::now();
    let mut warmup_iters = 0;
    while warmup_start.elapsed() < Duration::from_millis(50) && warmup_iters < 100 {
        f();
        warmup_iters += 1;
    }

    // Measurement
    let mut latencies = Vec::with_capacity(10_000);
    let start = Instant::now();
    let mut iterations = 0;

    while start.elapsed() < target_duration || iterations < 20 {
        let op_start = Instant::now();
        f();
        let op_duration = op_start.elapsed();
        latencies.push(op_duration);
        iterations += 1;
        if iterations >= 100_000 {
            break;
        }
    }
    let total_time = start.elapsed();

    latencies.sort();
    let min_time = latencies[0];
    let mean_time = total_time / (iterations as u32);
    let median_time = latencies[latencies.len() / 2];
    let p95_time = latencies[(latencies.len() as f64 * 0.95) as usize];
    let p99_time = latencies[(latencies.len() as f64 * 0.99).min((latencies.len() - 1) as f64) as usize];
    let ops_per_sec = (iterations as f64) / total_time.as_secs_f64();

    let throughput_mb_s = bytes_per_op.map(|b| {
        let total_bytes = b * iterations;
        (total_bytes as f64 / (1024.0 * 1024.0)) / total_time.as_secs_f64()
    });

    BenchResult {
        category,
        name,
        iterations,
        total_time,
        min_time,
        mean_time,
        median_time,
        p95_time,
        p99_time,
        ops_per_sec,
        throughput_mb_s,
    }
}

fn format_duration(d: Duration) -> String {
    let nanos = d.as_nanos();
    if nanos < 1_000 {
        format!("{nanos} ns")
    } else if nanos < 1_000_000 {
        format!("{:.2} µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.2} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

fn format_ops(ops: f64) -> String {
    if ops >= 1_000_000.0 {
        format!("{:.2} M ops/s", ops / 1_000_000.0)
    } else if ops >= 1_000.0 {
        format!("{:.2} K ops/s", ops / 1_000.0)
    } else {
        format!("{:.1} ops/s", ops)
    }
}

fn create_temp_file(lines_count: usize) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    for i in 0..lines_count {
        writeln!(
            file,
            "line {i:06}: let result = compute_value_{}(param_{}, &mut state); // sample code line with unicode ✨",
            i % 10,
            i % 50
        )
        .unwrap();
    }
    file.flush().unwrap();
    file
}

fn main() {
    println!("\x1b[1;36m========================================================================================\x1b[0m");
    println!("\x1b[1;37m                       YONRO TEXT EDITOR BENCHMARK SUITE                                \x1b[0m");
    println!("\x1b[1;36m========================================================================================\x1b[0m\n");

    let duration = Duration::from_millis(150);
    let mut results: Vec<BenchResult> = Vec::new();

    // 1. Line Core Benchmarks
    println!("\x1b[1;33m[1/7] Benchmarking Line & Grapheme Core Engine...\x1b[0m");
    let short_str = "fn main() { println!(\"hello\"); }";
    results.push(run_benchmark("Line", "from_str (ASCII short 33B)", Some(short_str.len()), duration, || {
        let _ = Line::from(short_str);
    }));

    let unicode_str = "नमस्ते दुनिया! こんにちは世界！ Здравствуйте, мир! Hello 🚀🦀✨🔥";
    results.push(run_benchmark("Line", "from_str (Unicode Multilingual 98B)", Some(unicode_str.len()), duration, || {
        let _ = Line::from(unicode_str);
    }));

    let emoji_str = "👨‍👩‍👧‍👦 👨‍💻 🏳️‍🌈 👩🏼‍🚀 🏃‍♀️ 🧙‍♂️ 🧟‍♂️ 🧝‍♀️ 🦸‍♂️ 🧑‍🍳 ".repeat(10);
    results.push(run_benchmark("Line", "from_str (Complex Emojis with ZWJ)", Some(emoji_str.len()), duration, || {
        let _ = Line::from(&emoji_str);
    }));

    let base_line = Line::from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
    results.push(run_benchmark("Line", "insert_char (middle)", None, duration, || {
        let mut l = base_line.clone();
        l.insert_char('X', 26);
    }));

    results.push(run_benchmark("Line", "delete_char (middle)", None, duration, || {
        let mut l = base_line.clone();
        l.delete(25);
    }));

    results.push(run_benchmark("Line", "split_line (middle)", None, duration, || {
        let mut l = base_line.clone();
        let _ = l.split(26);
    }));

    let search_line = Line::from("pub fn find_all(&self, query: &str, range: Range<ByteIdx>) -> Vec<(ByteIdx, GraphemeIdx)> { let query = query.trim(); }");
    results.push(run_benchmark("Line", "search_forward (pattern hit)", None, duration, || {
        let _ = search_line.search_forward("ByteIdx", 0);
    }));

    results.push(run_benchmark("Line", "get_visible_graphemes (viewport slice)", None, duration, || {
        let _ = search_line.get_visible_graphemes(10..40);
    }));

    // 2. Buffer Engine Benchmarks
    println!("\x1b[1;33m[2/7] Benchmarking Buffer Operations & Storage...\x1b[0m");
    let temp_100 = create_temp_file(100);
    let path_100 = temp_100.path().to_str().unwrap();
    let bytes_100 = std::fs::metadata(path_100).unwrap().len() as usize;
    results.push(run_benchmark("Buffer", "load (100 lines)", Some(bytes_100), duration, || {
        let _ = Buffer::load(path_100).unwrap();
    }));

    let temp_1k = create_temp_file(1_000);
    let path_1k = temp_1k.path().to_str().unwrap();
    let bytes_1k = std::fs::metadata(path_1k).unwrap().len() as usize;
    results.push(run_benchmark("Buffer", "load (1,000 lines)", Some(bytes_1k), duration, || {
        let _ = Buffer::load(path_1k).unwrap();
    }));

    let temp_5k = create_temp_file(5_000);
    let path_5k = temp_5k.path().to_str().unwrap();
    let bytes_5k = std::fs::metadata(path_5k).unwrap().len() as usize;
    results.push(run_benchmark("Buffer", "load (5,000 lines)", Some(bytes_5k), duration, || {
        let _ = Buffer::load(path_5k).unwrap();
    }));

    results.push(run_benchmark("Buffer", "insert_100_chars_typing", None, duration, || {
        let mut buffer = Buffer::default();
        for i in 0..100 {
            buffer.insert_char('x', Location { line_idx: 0, grapheme_idx: i });
        }
    }));

    results.push(run_benchmark("Buffer", "insert_50_newlines", None, duration, || {
        let mut buffer = Buffer::default();
        for i in 0..50 {
            buffer.insert_newline(Location { line_idx: i, grapheme_idx: 0 });
        }
    }));

    let buf_search = Buffer::load(path_1k).unwrap();
    results.push(run_benchmark("Buffer", "search_forward (across 1k lines)", None, duration, || {
        let _ = buf_search.search_forward("line 000950", Location { line_idx: 0, grapheme_idx: 0 });
    }));

    results.push(run_benchmark("Buffer", "lines_as_strings (1k lines snapshot)", None, duration, || {
        let _ = buf_search.lines_as_strings();
    }));

    // 3. Syntax Highlighting Benchmarks
    println!("\x1b[1;33m[3/7] Benchmarking Syntax Highlighting & Annotated Strings...\x1b[0m");
    let rust_code = Line::from("pub fn calculate<'a>(input: &'a str, factor: f64) -> Result<Option<usize>, &'static str> { let x = 0xDEAD_BEEF; }");
    results.push(run_benchmark("Syntax", "RustSyntaxHighlighter (single line)", None, duration, || {
        let mut highlighter = RustSyntaxHighlighter::default();
        highlighter.highlight(0, &rust_code);
    }));

    let sample_rust_lines: Vec<Line> = (0..50).map(|i| {
        Line::from(&format!("pub fn func_{i}(val: i32) -> Result<Option<usize>, Error> {{ if val > 0 {{ Ok(Some({i})) }} else {{ Err(Error) }} }}"))
    }).collect();

    results.push(run_benchmark("Syntax", "Rust document highlighting (50 lines)", None, duration, || {
        let mut highlighter = RustSyntaxHighlighter::default();
        for (idx, line) in sample_rust_lines.iter().enumerate() {
            highlighter.highlight(idx, line);
        }
    }));

    let sample_md_lines: Vec<Line> = vec![
        Line::from("# Heading 1"),
        Line::from("## Heading 2 with **bold** and *italic* and `code`"),
        Line::from("- List item 1"),
        Line::from("- List item 2 with [link](https://example.com)"),
        Line::from("```rust"),
        Line::from("fn main() { println!(\"hello\"); }"),
        Line::from("```"),
    ];

    results.push(run_benchmark("Syntax", "Markdown document highlighting", None, duration, || {
        let mut highlighter = MarkDownSyntaxHighlighter::default();
        for (idx, line) in sample_md_lines.iter().enumerate() {
            highlighter.highlight(idx, line);
        }
    }));

    let mut ann_str = AnnotatedString::from("pub fn process_data(value: usize) -> Option<String> {");
    ann_str.add_annotation(AnnotationType::Keyword, 0, 3);
    ann_str.add_annotation(AnnotationType::Keyword, 4, 6);
    ann_str.add_annotation(AnnotationType::Type, 24, 29);
    results.push(run_benchmark("Syntax", "AnnotatedString slice & iterate", None, duration, || {
        let parts: Vec<_> = (&ann_str).into_iter().collect();
        std::hint::black_box(parts);
    }));

    results.push(run_benchmark("Syntax", "AnnotatedString replace & shift", None, duration, || {
        let mut s = ann_str.clone();
        s.replace(7, 19, "transformed_name");
    }));

    // 4. Layout & Panes
    println!("\x1b[1;33m[4/7] Benchmarking Layout Tree & Pane Management...\x1b[0m");
    let screen_rect = Rect { position: Position { row: 1, col: 0 }, size: Size { height: 60, width: 200 } };
    let mut tree_4 = LayoutTree::new(0, screen_rect);
    let _ = tree_4.split_pane(0, 1, SplitDirection::Vertical, 0.5);
    let _ = tree_4.split_pane(0, 2, SplitDirection::Horizontal, 0.5);
    let _ = tree_4.split_pane(1, 3, SplitDirection::Horizontal, 0.5);

    results.push(run_benchmark("Layout", "compute_layout (4 panes 2x2)", None, duration, || {
        tree_4.compute_layout(screen_rect);
        let _ = tree_4.collect_leaf_layouts();
    }));

    let mut tree_16 = LayoutTree::new(0, screen_rect);
    for i in 1..16 {
        let parent = (i - 1) / 2;
        let dir = if i % 2 == 0 { SplitDirection::Horizontal } else { SplitDirection::Vertical };
        let _ = tree_16.split_pane(parent, i, dir, 0.5);
    }

    results.push(run_benchmark("Layout", "compute_layout (16 panes deep tree)", None, duration, || {
        tree_16.compute_layout(screen_rect);
        let _ = tree_16.collect_leaf_layouts();
    }));

    results.push(run_benchmark("Layout", "find_split_at hit test (16 panes)", None, duration, || {
        let _ = tree_16.find_split(Position { row: 30, col: 100 });
    }));

    results.push(run_benchmark("Layout", "resize_split (16 panes)", None, duration, || {
        tree_16.resize_split(1, Position { row: 30, col: 120 });
    }));

    let init_pane = Pane {
        pane_id: 0,
        content: PaneContent::TextView(View::default()),
        active: true,
        is_floating: false,
        z_index: 0,
        is_minimized: false,
        rect: screen_rect,
    };
    let mut pane_mgr = PaneManager::new(init_pane);
    for i in 1..=8 {
        pane_mgr.create_floating_pane(PaneContent::TextView(View::default()), i);
    }
    results.push(run_benchmark("Layout", "PaneManager z-index sort & promote", None, duration, || {
        pane_mgr.bring_to_front(3);
        let _ = pane_mgr.get_floating_panes_sorted();
    }));

    // 5. Command Dispatcher & View
    println!("\x1b[1;33m[5/7] Benchmarking Command Dispatcher & View Operations...\x1b[0m");
    results.push(run_benchmark("View", "Edit + 50 Undo + 50 Redo cycle", None, duration, || {
        let mut view = View::default();
        let mut buffer = Buffer::default();
        for ch in "fn test_function_with_long_identifier() { return; }".chars() {
            view.handle_edit_command(Edit::Insert(ch), &mut buffer);
        }
        for _ in 0..50 { view.undo(&mut buffer); }
        for _ in 0..50 { view.redo(&mut buffer); }
    }));

    let mut nav_view = View::default();
    let mut nav_buf = Buffer::default();
    for i in 0..100 {
        for ch in format!("line {i} code here\n").chars() {
            if ch == '\n' { nav_view.handle_edit_command(Edit::InsertNewLine, &mut nav_buf); }
            else { nav_view.handle_edit_command(Edit::Insert(ch), &mut nav_buf); }
        }
    }
    results.push(run_benchmark("View", "cursor_movement across 100 lines", None, duration, || {
        for _ in 0..20 { nav_view.handle_move_command(Move::Down, &nav_buf); }
        nav_view.handle_move_command(Move::EndOfLine, &nav_buf);
        for _ in 0..20 { nav_view.handle_move_command(Move::Up, &nav_buf); }
        nav_view.handle_move_command(Move::StartOfLine, &nav_buf);
    }));

    results.push(run_benchmark("View", "caret_position coordinate mapping", None, duration, || {
        let _ = nav_view.caret_position(&nav_buf);
    }));

    let mut buf_mgr = BufferManager::new();
    let b_id = buf_mgr.add(Buffer::default());
    let mut initial_v = View::default();
    initial_v.set_buffer_id(b_id);
    let p_0 = Pane { pane_id: 0, content: PaneContent::TextView(initial_v), active: true, is_floating: false, z_index: 0, is_minimized: false, rect: screen_rect };
    let mut p_mgr = PaneManager::new(p_0);
    let mut l_tree = LayoutTree::new(0, screen_rect);
    let mut b_bar = BufferBar::default();
    let mut c_bar = CommandBar::default();
    let mut m_bar = MessageBar::default();
    let mut p_type = PromptType::None;
    let mut s_quit = false;
    let mut q_times = 0;
    let mut d_split = None;
    let mut d_pane = None;
    let mut d_offset = Position::default();
    let mut reg = HandlerRegistry::default();
    let cmd_down = Command::Move(Move::Down);

    results.push(run_benchmark("Command", "HandlerRegistry dispatch overhead", None, duration, || {
        let mut ctx = EditorContext {
            prompt_type: &mut p_type,
            pane_manager: &mut p_mgr,
            layout_tree: &mut l_tree,
            buffer_manager: &mut buf_mgr,
            buffer_bar: &mut b_bar,
            command_bar: &mut c_bar,
            message_bar: &mut m_bar,
            terminal_size: screen_rect.size,
            should_quit: &mut s_quit,
            quit_times: &mut q_times,
            dragging_split: &mut d_split,
            dragging_pane: &mut d_pane,
            drag_offset: &mut d_offset,
            buffer_changed: None,
        };
        let _ = reg.dispatch(&cmd_down, &mut ctx);
    }));

    // 6. Plugin Runtime
    println!("\x1b[1;33m[6/7] Benchmarking Plugin Runtime & Event Messaging...\x1b[0m");
    let plugin_rt = PluginRuntime::new();
    plugin_rt.load_plugin(Box::new(FileExplorerPlugin::new()));
    results.push(run_benchmark("Plugin", "event_send & response_drain", None, duration, || {
        plugin_rt.send(PluginMessage::Event {
            event: EditorEvent::Custom(yonro_core::events::customevent::CustomEvent::ThemeChanged),
            active_pane_id: 0,
        });
        let _ = plugin_rt.drain_responses();
    }));

    let snap_buf = Buffer::load(path_100).unwrap();
    results.push(run_benchmark("Plugin", "BufferSnapshot delivery (100 lines)", None, duration, || {
        let snapshot = BufferSnapshot {
            buffer_id: 1,
            lines: snap_buf.lines_as_strings(),
            file_name: Some("main.rs".to_string()),
            is_dirty: snap_buf.is_dirty(),
        };
        plugin_rt.send(PluginMessage::BufferChanged(snapshot));
        let _ = plugin_rt.drain_responses();
    }));
    plugin_rt.shutdown();

    // 7. End-to-End Macro Workflows
    println!("\x1b[1;33m[7/7] Benchmarking Full End-to-End Macro Workflows...\x1b[0m");
    results.push(run_benchmark("E2E", "Session: Type 50 lines + Search + Edit + Highlight", None, duration, || {
        let mut b_mgr = BufferManager::new();
        let mut buf = Buffer::default();
        for line_idx in 0..50 {
            let code = format!("pub fn compute_step_{line_idx}(val: i32) -> Result<i32, Error> {{ Ok(val * 2) }}");
            for ch in code.chars() {
                buf.insert_char(ch, Location { line_idx, grapheme_idx: buf.grapheme_count(line_idx) });
            }
            buf.insert_newline(Location { line_idx, grapheme_idx: buf.grapheme_count(line_idx) });
        }
        let b_id = b_mgr.add(buf);
        let mut view = View::default();
        view.set_buffer_id(b_id);
        view.set_size(screen_rect);

        let buf_ref = b_mgr.get(b_id).unwrap();
        let _ = buf_ref.search_forward("compute_step_25", Location { line_idx: 0, grapheme_idx: 0 });

        let mut buf_mut = b_mgr.get_mut(b_id).unwrap();
        view.handle_edit_command(Edit::Insert('X'), &mut buf_mut);

        let mut highlighter = RustSyntaxHighlighter::default();
        for idx in 0..30 {
            if let Some(line) = buf_mut.get_line(idx) {
                highlighter.highlight(idx, &line);
            }
        }
        let _ = view.caret_position(buf_mut);
    }));

    results.push(run_benchmark("E2E", "Multi-Pane: 4-Split + Multi-Buffer Edits + Layout Sync", None, duration, || {
        let mut b_mgr = BufferManager::new();
        let b1 = b_mgr.add(Buffer::default());
        let b2 = b_mgr.add(Buffer::default());
        let b3 = b_mgr.add(Buffer::default());
        let b4 = b_mgr.add(Buffer::default());

        let mut tree = LayoutTree::new(0, screen_rect);
        let _ = tree.split_pane(0, 1, SplitDirection::Vertical, 0.5);
        let _ = tree.split_pane(0, 2, SplitDirection::Horizontal, 0.5);
        let _ = tree.split_pane(1, 3, SplitDirection::Horizontal, 0.5);

        let mut v0 = View::default(); v0.set_buffer_id(b1);
        let mut v1 = View::default(); v1.set_buffer_id(b2);
        let mut v2 = View::default(); v2.set_buffer_id(b3);
        let mut v3 = View::default(); v3.set_buffer_id(b4);

        let p0 = Pane { pane_id: 0, content: PaneContent::TextView(v0), active: true, is_floating: false, z_index: 0, is_minimized: false, rect: Rect::default() };
        let mut p_mgr = PaneManager::new(p0);
        p_mgr.create_pane(PaneContent::TextView(v1));
        p_mgr.create_pane(PaneContent::TextView(v2));
        p_mgr.create_pane(PaneContent::TextView(v3));

        tree.compute_layout(screen_rect);
        for (id, rect) in tree.collect_leaf_layouts() {
            if let Some(pane) = p_mgr.get_pane_mut(id) {
                pane.resize(rect);
            }
        }

        for id in [0, 1, 2, 3] {
            p_mgr.set_active_pane(id);
            if let Some(pane) = p_mgr.get_pane_mut(id) {
                if let Some(view) = pane.view_mut() {
                    let buf_id = view.buffer_id();
                    if let Some(buf) = b_mgr.get_mut(buf_id) {
                        view.handle_edit_command(Edit::Insert('A'), buf);
                    }
                }
            }
        }
    }));

    // PRINT TABLE
    println!("\n\x1b[1;32m========================================================================================================================\x1b[0m");
    println!("\x1b[1;37m                                               BENCHMARK RESULTS REPORT                                                 \x1b[0m");
    println!("\x1b[1;32m========================================================================================================================\x1b[0m");
    println!(
        "\x1b[1;34m{:<9} {:<42} {:>10} {:>10} {:>10} {:>10} {:>14} {:>14}\x1b[0m",
        "Category", "Benchmark Target", "Mean", "Median", "P95", "P99", "Throughput", "Rate (ops/s)"
    );
    println!("------------------------------------------------------------------------------------------------------------------------");

    let mut current_cat = "";
    for r in &results {
        if r.category != current_cat {
            current_cat = r.category;
            println!("\x1b[1;35m--- {} ---\x1b[0m", current_cat);
        }

        let tp = r.throughput_mb_s.map(|mb| format!("{:.1} MB/s", mb)).unwrap_or_else(|| "-".to_string());

        println!(
            "{:<9} {:<42} {:>10} {:>10} {:>10} {:>10} {:>14} {:>14}",
            r.category,
            r.name,
            format_duration(r.mean_time),
            format_duration(r.median_time),
            format_duration(r.p95_time),
            format_duration(r.p99_time),
            tp,
            format_ops(r.ops_per_sec)
        );
    }
    println!("========================================================================================================================");
    println!("\x1b[1;32mTotal benchmarks executed: {}\x1b[0m", results.len());
    println!("\x1b[1;36mAll subsystems (Line, Buffer, Syntax, Layout, Command, View, Plugin, E2E) benchmarked successfully!\x1b[0m\n");
}
