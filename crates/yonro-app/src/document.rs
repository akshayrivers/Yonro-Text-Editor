//! The manuscript page. Core owns the rope and the row breaks. This view paints.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{
    canvas, div, fill, font, point, px, rgb, size, App, Bounds, Context, IntoElement, Pixels,
    Point, Render, SharedString, TextRun, Window,
};
use yonro_core::{grapheme_byte, prelude::Location, row_text, visual_rows, Buffer};

use crate::theme::{DESK, INK, PAPER, QUIET};

const COLUMNS: usize = 70;
const FONT_PX: f32 = 20.0;
const LINE_PX: f32 = 34.0;

struct PaintedRow {
    text: String,
    line_idx: usize,
    start: usize,
}

pub struct Page {
    buffer: Buffer,
    /// Scene path, used when the page learns to save.
    #[allow(dead_code)]
    path: Option<PathBuf>,
    caret: Location,
}

impl Page {
    /// Open a scene file, or an empty page when `path` is missing.
    pub fn open(path: Option<PathBuf>) -> Self {
        let (buffer, path) = load(path);
        Self {
            buffer,
            path,
            caret: Location::default(),
        }
    }
}

impl Render for Page {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let rows = painted_rows(&self.buffer);
        let empty = self.buffer.is_empty();
        let caret = self.caret;
        let mut height = px(0.0);
        for _ in 0..rows.len().max(1) {
            height += px(LINE_PX);
        }
        div()
            .size_full()
            .bg(rgb(DESK))
            .flex()
            .justify_center()
            .child(
                div()
                    .id("sheet")
                    .w(px(760.0))
                    .h_full()
                    .bg(rgb(PAPER))
                    .overflow_y_scroll()
                    .p_8()
                    .child(
                        canvas(
                            move |_bounds, _window, _cx| (rows, empty, caret),
                            move |bounds, (rows, empty, caret), window, cx| {
                                paint_page(bounds.origin, &rows, empty, caret, window, cx);
                            },
                        )
                        .w_full()
                        .h(height),
                    ),
            )
    }
}

fn paint_page(
    origin: Point<Pixels>,
    rows: &[PaintedRow],
    empty: bool,
    caret: Location,
    window: &mut Window,
    cx: &mut App,
) {
    let face = font("Georgia");
    let font_size = px(FONT_PX);
    let line_height = px(LINE_PX);
    if empty {
        paint_line(
            origin,
            "start the draft here.",
            &PaintStyle {
                face: face.clone(),
                color: rgb(QUIET).into(),
                font_size,
                line_height,
            },
            window,
            cx,
        );
    }
    let mut y = px(0.0);
    for row in rows {
        if empty {
            break;
        }
        let mut at = origin;
        at.y += y;
        let shaped = paint_line(
            at,
            &row.text,
            &PaintStyle {
                face: face.clone(),
                color: rgb(INK).into(),
                font_size,
                line_height,
            },
            window,
            cx,
        );
        if row.line_idx == caret.line_idx && caret.grapheme_idx >= row.start {
            let local = caret.grapheme_idx.saturating_sub(row.start);
            let byte = grapheme_byte_in_row(&row.text, local);
            let x = shaped.x_for_index(byte);
            window.paint_quad(fill(
                Bounds {
                    origin: point(origin.x + x, at.y),
                    size: size(px(1.0), px(LINE_PX - 6.0)),
                },
                rgb(INK),
            ));
        }
        y += line_height;
    }
    if empty {
        window.paint_quad(fill(
            Bounds {
                origin,
                size: size(px(1.0), px(LINE_PX - 6.0)),
            },
            rgb(INK),
        ));
    }
}

struct PaintStyle {
    face: gpui::Font,
    color: gpui::Hsla,
    font_size: Pixels,
    line_height: Pixels,
}

fn paint_line(
    origin: Point<Pixels>,
    text: &str,
    style: &PaintStyle,
    window: &mut Window,
    cx: &mut App,
) -> gpui::ShapedLine {
    let shown = if text.is_empty() { " " } else { text };
    let run = TextRun {
        len: shown.len(),
        font: style.face.clone(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        SharedString::from(shown.to_owned()),
        style.font_size,
        &[run],
        None,
    );
    let _ = shaped.paint(origin, style.line_height, window, cx);
    shaped
}

fn grapheme_byte_in_row(text: &str, grapheme_idx: usize) -> usize {
    grapheme_byte(text, grapheme_idx)
}

fn painted_rows(buffer: &Buffer) -> Vec<PaintedRow> {
    visual_rows(buffer, COLUMNS)
        .into_iter()
        .map(|row| {
            let text = row_text(buffer, &row);
            PaintedRow {
                text,
                line_idx: row.line_idx,
                start: row.start,
            }
        })
        .collect()
}

fn load(path: Option<PathBuf>) -> (Buffer, Option<PathBuf>) {
    let Some(path) = path else {
        return (Buffer::default(), None);
    };
    let file = if path.is_dir() {
        first_scene(&path)
    } else if path.is_file() {
        Some(path)
    } else {
        None
    };
    let Some(file) = file else {
        return (Buffer::default(), None);
    };
    match Buffer::load(&file.to_string_lossy()) {
        Ok(buffer) => (buffer, Some(file)),
        Err(_) => (Buffer::default(), Some(file)),
    }
}

fn first_scene(dir: &Path) -> Option<PathBuf> {
    let mut names = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("scene-"))
        })
        .collect::<Vec<_>>();
    names.sort();
    names.into_iter().next()
}
