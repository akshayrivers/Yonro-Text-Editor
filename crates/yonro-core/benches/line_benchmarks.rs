use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use yonro_core::Line;

fn bench_line_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_creation");

    let short_ascii = "fn main() { println!(\"hello\"); }";
    let medium_ascii = "pub fn handle_resize_command(&mut self, size: Size) { self.terminal_size = size; self.sync(); }";
    let long_ascii = "let very_long_string_with_many_identifiers_and_tokens_for_benchmarking_purposes = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]; ".repeat(10);
    let unicode_text =
        "नमस्ते दुनिया! こんにちは世界！ مرحبا بالعالم Здравствуйте, мир! Hello 🚀🦀✨🔥";
    let complex_emoji = "👨‍👩‍👧‍👦 👨‍💻 🏳️‍🌈 👩🏼‍🚀 🏃‍♀️ 🧙‍♂️ 🧟‍♂️ 🧝‍♀️ 🦸‍♂️ 🧑‍🍳 ".repeat(10);

    for (name, text) in [
        ("short_ascii", short_ascii),
        ("medium_ascii", medium_ascii),
        ("long_ascii", &long_ascii),
        ("unicode_multilingual", unicode_text),
        ("complex_emoji_zwj", &complex_emoji),
    ] {
        group.throughput(Throughput::Bytes(text.len() as u64));
        group.bench_with_input(BenchmarkId::new("from_str", name), &text, |b, &text| {
            b.iter(|| Line::from(black_box(text)));
        });
    }
    group.finish();
}

fn bench_line_mutations(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_mutations");

    // Insert char
    group.bench_function("insert_char_middle_ascii", |b| {
        let base = Line::from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        b.iter(|| {
            let mut line = base.clone();
            line.insert_char(black_box('X'), black_box(26));
            black_box(line);
        });
    });

    group.bench_function("insert_char_unicode", |b| {
        let base = Line::from("नमस्ते दुनिया! 🚀🦀✨");
        b.iter(|| {
            let mut line = base.clone();
            line.insert_char(black_box('🔥'), black_box(5));
            black_box(line);
        });
    });

    group.bench_function("append_char", |b| {
        let base = Line::from("let x = 42;");
        b.iter(|| {
            let mut line = base.clone();
            line.append_char(black_box(' '));
            black_box(line);
        });
    });

    // Delete char
    group.bench_function("delete_char_middle", |b| {
        let base = Line::from("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ");
        b.iter(|| {
            let mut line = base.clone();
            line.delete(black_box(25));
            black_box(line);
        });
    });

    group.bench_function("delete_last_char", |b| {
        let base = Line::from("abcdefghijklmnopqrstuvwxyz");
        b.iter(|| {
            let mut line = base.clone();
            line.delete_last();
            black_box(line);
        });
    });

    // Split line
    group.bench_function("split_line_middle", |b| {
        let base = Line::from("fn calculate_something_very_important(param1: i32, param2: &str) -> Result<usize, Error> {");
        b.iter(|| {
            let mut line = base.clone();
            let remainder = line.split(black_box(45));
            black_box((line, remainder));
        });
    });

    // Append lines
    group.bench_function("append_lines", |b| {
        let base1 = Line::from("first_part_of_the_code_line_here = ");
        let base2 = Line::from("second_part_with_function_call();");
        b.iter(|| {
            let mut line = base1.clone();
            line.append(black_box(&base2));
            black_box(line);
        });
    });

    group.finish();
}

fn bench_grapheme_and_width(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_width_and_visible");

    let mixed_line = Line::from("let greeting = \"नमस्ते दुनिया! 你好世界 👨‍💻🚀\"; // Comments here");

    group.bench_function("grapheme_count", |b| {
        b.iter(|| black_box(&mixed_line).grapheme_count());
    });

    group.bench_function("width_full", |b| {
        b.iter(|| black_box(&mixed_line).width());
    });

    group.bench_function("width_until_half", |b| {
        let half = mixed_line.grapheme_count() / 2;
        b.iter(|| black_box(&mixed_line).width_until(black_box(half)));
    });

    group.bench_function("get_visible_graphemes_viewport", |b| {
        b.iter(|| black_box(&mixed_line).get_visible_graphemes(black_box(10..40)));
    });

    group.bench_function("get_annotated_visible_substr", |b| {
        b.iter(|| black_box(&mixed_line).get_annotated_visible_substr(black_box(5..50), None));
    });

    group.finish();
}

fn bench_line_search(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_search");

    let haystack = Line::from("pub fn find_all(&self, query: &str, range: Range<ByteIdx>) -> Vec<(ByteIdx, GraphemeIdx)> { let query = query.trim(); }");

    group.bench_function("search_forward_hit", |b| {
        b.iter(|| black_box(&haystack).search_forward(black_box("ByteIdx"), black_box(0)));
    });

    group.bench_function("search_forward_miss", |b| {
        b.iter(|| {
            black_box(&haystack).search_forward(black_box("nonexistent_pattern"), black_box(0))
        });
    });

    group.bench_function("search_backward_hit", |b| {
        let end = haystack.grapheme_count();
        b.iter(|| black_box(&haystack).search_backward(black_box("query"), black_box(end)));
    });

    group.bench_function("find_all_matches", |b| {
        let len = haystack.len();
        b.iter(|| black_box(&haystack).find_all(black_box("query"), black_box(0..len)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_line_creation,
    bench_line_mutations,
    bench_grapheme_and_width,
    bench_line_search
);
criterion_main!(benches);
