use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::io::Write;
use tempfile::NamedTempFile;
use yonro_core::prelude::*;
use yonro_core::Buffer;

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

fn bench_buffer_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_load");

    for &size in &[100, 1_000, 5_000] {
        let temp = create_temp_file(size);
        let path = temp.path().to_str().unwrap();
        let file_bytes = std::fs::metadata(path).unwrap().len();

        group.throughput(Throughput::Bytes(file_bytes));
        group.bench_with_input(BenchmarkId::new("load_lines", size), &path, |b, &path| {
            b.iter(|| {
                let buffer = Buffer::load(black_box(path)).unwrap();
                black_box(buffer);
            });
        });
    }
    group.finish();
}

fn bench_buffer_editing(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_editing");

    group.bench_function("insert_100_chars_sequential", |b| {
        b.iter(|| {
            let mut buffer = Buffer::default();
            for i in 0..100 {
                buffer.insert_char(
                    'x',
                    Location {
                        line_idx: 0,
                        grapheme_idx: i,
                    },
                );
            }
            black_box(buffer);
        });
    });

    group.bench_function("insert_50_newlines", |b| {
        b.iter(|| {
            let mut buffer = Buffer::default();
            for i in 0..50 {
                buffer.insert_newline(Location {
                    line_idx: i,
                    grapheme_idx: 0,
                });
            }
            black_box(buffer);
        });
    });

    group.bench_function("delete_chars_backward", |b| {
        let mut base = Buffer::default();
        for i in 0..100 {
            base.insert_char(
                'a',
                Location {
                    line_idx: 0,
                    grapheme_idx: i,
                },
            );
        }
        b.iter(|| {
            let mut buffer = Buffer::default();
            for i in 0..100 {
                buffer.insert_char(
                    'a',
                    Location {
                        line_idx: 0,
                        grapheme_idx: i,
                    },
                );
            }
            for i in (0..100).rev() {
                buffer.delete(Location {
                    line_idx: 0,
                    grapheme_idx: i,
                });
            }
            black_box(buffer);
        });
    });

    group.bench_function("line_merge_via_delete", |b| {
        b.iter(|| {
            let mut buffer = Buffer::default();
            buffer.insert_char(
                'a',
                Location {
                    line_idx: 0,
                    grapheme_idx: 0,
                },
            );
            buffer.insert_newline(Location {
                line_idx: 0,
                grapheme_idx: 1,
            });
            buffer.insert_char(
                'b',
                Location {
                    line_idx: 1,
                    grapheme_idx: 0,
                },
            );
            // Delete at end of line 0 merges line 1 into line 0
            buffer.delete(Location {
                line_idx: 0,
                grapheme_idx: 1,
            });
            black_box(buffer);
        });
    });

    group.finish();
}

fn bench_buffer_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_search");

    let temp = create_temp_file(2_000);
    let buffer = Buffer::load(temp.path().to_str().unwrap()).unwrap();

    group.bench_function("search_forward_hit_early", |b| {
        b.iter(|| {
            buffer.search_forward(
                black_box("line 000010"),
                Location {
                    line_idx: 0,
                    grapheme_idx: 0,
                },
            )
        });
    });

    group.bench_function("search_forward_hit_late", |b| {
        b.iter(|| {
            buffer.search_forward(
                black_box("line 001950"),
                Location {
                    line_idx: 0,
                    grapheme_idx: 0,
                },
            )
        });
    });

    group.bench_function("search_forward_miss", |b| {
        b.iter(|| {
            buffer.search_forward(
                black_box("nonexistent_needle_404"),
                Location {
                    line_idx: 0,
                    grapheme_idx: 0,
                },
            )
        });
    });

    group.bench_function("search_backward_hit", |b| {
        b.iter(|| {
            buffer.search_backward(
                black_box("line 000100"),
                Location {
                    line_idx: 1999,
                    grapheme_idx: 10,
                },
            )
        });
    });

    group.finish();
}

fn bench_buffer_snapshot(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_snapshot");

    let temp_1k = create_temp_file(1_000);
    let buffer_1k = Buffer::load(temp_1k.path().to_str().unwrap()).unwrap();

    group.bench_function("lines_as_strings_1k", |b| {
        b.iter(|| {
            black_box(&buffer_1k).lines_as_strings();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_buffer_load,
    bench_buffer_editing,
    bench_buffer_search,
    bench_buffer_snapshot
);
criterion_main!(benches);
