use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yonro_core::{events::EditorEvent, Buffer};
use yonro_tui::plugins::{BufferSnapshot, FileExplorerPlugin, PluginMessage, PluginRuntime};

fn bench_plugin_runtime_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("plugin_runtime");

    group.bench_function("plugin_message_send_and_drain", |b| {
        let runtime = PluginRuntime::new();
        runtime.load_plugin(Box::new(FileExplorerPlugin::new()));

        b.iter(|| {
            runtime.send(PluginMessage::Event {
                event: EditorEvent::Custom(yonro_text_editor::editor::events::customevent::CustomEvent::ThemeChanged),
                active_pane_id: 0,
            });
            let responses = runtime.drain_responses();
            black_box(responses);
        });

        runtime.shutdown();
    });

    group.bench_function("buffer_snapshot_creation_and_delivery", |b| {
        let runtime = PluginRuntime::new();
        runtime.load_plugin(Box::new(FileExplorerPlugin::new()));

        let mut buffer = Buffer::default();
        for i in 0..100 {
            buffer.insert_char('a', yonro_text_editor::prelude::Location { line_idx: 0, grapheme_idx: i });
        }

        b.iter(|| {
            let snapshot = BufferSnapshot {
                buffer_id: 1,
                lines: buffer.lines_as_strings(),
                file_name: Some("test.rs".to_string()),
                is_dirty: buffer.is_dirty(),
            };
            runtime.send(PluginMessage::BufferChanged(black_box(snapshot)));
            let responses = runtime.drain_responses();
            black_box(responses);
        });

        runtime.shutdown();
    });

    group.finish();
}

criterion_group!(benches, bench_plugin_runtime_throughput);
criterion_main!(benches);
