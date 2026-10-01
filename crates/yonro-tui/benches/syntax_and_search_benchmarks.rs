use criterion::{black_box, criterion_group, criterion_main, Criterion};
use yonro_core::{
    annotatedstring::AnnotatedString,
    annotationtype::AnnotationType,
    highlighter::{
        MarkDownSyntaxHighlighter, RustSyntaxHighlighter, SearchResultHighlighter,
        SyntaxHighlighter,
    },
    line::Line,
};
use yonro_core::prelude::*;

fn sample_rust_code_lines() -> Vec<Line> {
    let raw_code = r#"
// This is a comment explaining the module
pub struct Engine<T: Clone + 'static> {
    state: Option<T>,
    counter: usize,
    buffer: Vec<String>,
}

impl<T: Clone + 'static> Engine<T> {
    pub fn new(initial: T) -> Result<Self, Error> {
        let mut buffer = Vec::new();
        buffer.push(String::from("initialized"));
        /* Multi-line comment here
           spanning across multiple lines */
        let numeric_literal = 0xDEAD_BEEF;
        let float_val = 3.14159e-2;
        let is_ready = true;
        if is_ready {
            println!("Value: {}", numeric_literal);
        }
        Ok(Self {
            state: Some(initial),
            counter: 42,
            buffer,
        })
    }
}
"#;
    raw_code.lines().map(Line::from).collect()
}

fn sample_markdown_lines() -> Vec<Line> {
    let raw_md = r#"
# Heading 1: Project Overview
## Heading 2: Features and Architecture

This is a paragraph with **bold text**, *italic text*, and `inline code`.
Here is a link: [Yonro Editor](https://github.com/example/yonro).

### Key Features:
- Fast text editing with *Unicode* support
- Pane management with **split trees**
- Built-in syntax highlighting for `Rust` and `Markdown`
* Multi-line code blocks:

```rust
fn main() {
    println!("Hello World");
}
```

End of document.
"#;
    raw_md.lines().map(Line::from).collect()
}

fn bench_rust_syntax_highlighting(c: &mut Criterion) {
    let mut group = c.benchmark_group("rust_syntax_highlighting");
    let lines = sample_rust_code_lines();

    group.bench_function("highlight_single_line_complex", |b| {
        let line = Line::from("pub fn calculate<'a>(input: &'a str, factor: f64) -> Result<Option<usize>, &'static str> { /* comment */ }");
        b.iter(|| {
            let mut highlighter = RustSyntaxHighlighter::default();
            highlighter.highlight(0, black_box(&line));
            black_box(highlighter);
        });
    });

    group.bench_function("highlight_rust_document_30_lines", |b| {
        b.iter(|| {
            let mut highlighter = RustSyntaxHighlighter::default();
            for (idx, line) in lines.iter().enumerate() {
                highlighter.highlight(idx, line);
            }
            black_box(highlighter);
        });
    });

    group.finish();
}

fn bench_markdown_syntax_highlighting(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_syntax_highlighting");
    let lines = sample_markdown_lines();

    group.bench_function("highlight_markdown_document_25_lines", |b| {
        b.iter(|| {
            let mut highlighter = MarkDownSyntaxHighlighter::default();
            for (idx, line) in lines.iter().enumerate() {
                highlighter.highlight(idx, line);
            }
            black_box(highlighter);
        });
    });

    group.finish();
}

fn bench_search_result_highlighting(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_result_highlighting");
    let line = Line::from("let buffer = Buffer::new(); buffer.insert('a'); buffer.save(); // buffer manipulation");

    group.bench_function("highlight_search_occurrences", |b| {
        b.iter(|| {
            let mut search_highlighter = SearchResultHighlighter::new("buffer", Some(Location { line_idx: 0, grapheme_idx: 4 }));
            search_highlighter.highlight(0, black_box(&line));
            black_box(search_highlighter.get_annotations(0));
        });
    });

    group.finish();
}

fn bench_annotated_string_ops(c: &mut Criterion) {
    let mut group = c.benchmark_group("annotated_string_ops");

    group.bench_function("build_and_add_annotations", |b| {
        b.iter(|| {
            let mut s = AnnotatedString::from(black_box("pub fn process_data(value: usize) -> Option<String> {"));
            s.add_annotation(AnnotationType::Keyword, 0, 3);   // pub
            s.add_annotation(AnnotationType::Keyword, 4, 6);   // fn
            s.add_annotation(AnnotationType::Type, 24, 29);    // usize
            s.add_annotation(AnnotationType::Type, 34, 40);    // Option
            s.add_annotation(AnnotationType::Type, 41, 47);    // String
            black_box(s);
        });
    });

    group.bench_function("slice_and_iterate_annotated_string", |b| {
        let mut s = AnnotatedString::from("pub fn process_data(value: usize) -> Option<String> {");
        s.add_annotation(AnnotationType::Keyword, 0, 3);
        s.add_annotation(AnnotationType::Keyword, 4, 6);
        s.add_annotation(AnnotationType::Type, 24, 29);
        b.iter(|| {
            let parts: Vec<_> = (&s).into_iter().collect();
            black_box(parts);
        });
    });

    group.bench_function("replace_and_shift_annotations", |b| {
        b.iter(|| {
            let mut s = AnnotatedString::from("pub fn process_data(value: usize) -> Option<String> {");
            s.add_annotation(AnnotationType::Keyword, 0, 3);
            s.add_annotation(AnnotationType::Keyword, 4, 6);
            s.add_annotation(AnnotationType::Type, 24, 29);
            s.replace(7, 19, "transformed_func_name");
            black_box(s);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_rust_syntax_highlighting,
    bench_markdown_syntax_highlighting,
    bench_search_result_highlighting,
    bench_annotated_string_ops
);
criterion_main!(benches);
