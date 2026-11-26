//! The manuscript page. Core owns the rope and the row breaks. This view paints.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{
    canvas, div, fill, font, point, px, rgb, size, App, Bounds, Context, ElementInputHandler,
    FocusHandle, IntoElement, Pixels, Point, Render, SharedString, TextRun, Window,
};
use yonro_core::{
    grapheme_byte, location_to_point, prelude::Location, row_text, visual_rows, Buffer, EditLog,
};

use crate::theme::{DESK, INK, PAPER, QUIET, WASH};

pub(crate) const COLUMNS: usize = 70;
const FONT_PX: f32 = 20.0;
const LINE_PX: f32 = 34.0;

#[derive(Clone)]
pub(crate) struct PaintedRow {
    pub(crate) text: String,
    pub(crate) line_idx: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) marks: Vec<std::ops::Range<usize>>,
}

pub struct Page {
    pub(crate) buffer: Buffer,
    /// Scene path, used when the page learns to save.
    #[allow(dead_code)]
    pub(crate) path: Option<PathBuf>,
    pub(crate) caret: Location,
    pub(crate) anchor: Option<Location>,
    pub(crate) log: EditLog,
    pub(crate) focus: FocusHandle,
    /// Text just inserted, so a key and the input handler do not type it twice.
    pub(crate) echo: Option<String>,
    pub(crate) marked: Option<(Location, Location)>,
    pub(crate) origin: Point<Pixels>,
    pub(crate) dragging: bool,
    pub(crate) lore: yonro_core::LoreBook,
    pub(crate) find_open: bool,
    pub(crate) find_query: String,
    pub(crate) find_from: Location,
    pub(crate) find_focus: FocusHandle,
    pub(crate) mentions: Vec<String>,
    pub(crate) mention_at: Option<Location>,
    pub(crate) mention_pick: usize,
}

impl Page {
    /// Open a scene file, or an empty page when `path` is missing.
    pub fn open(path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let lore = lore_for(path.as_deref());
        let (buffer, path) = load(path);
        Self {
            buffer,
            path,
            caret: Location::default(),
            anchor: None,
            log: EditLog::new(),
            focus: cx.focus_handle(),
            echo: None,
            marked: None,
            origin: point(px(0.0), px(0.0)),
            dragging: false,
            lore,
            find_open: false,
            find_query: String::new(),
            find_from: Location::default(),
            find_focus: cx.focus_handle(),
            mentions: Vec::new(),
            mention_at: None,
            mention_pick: 0,
        }
    }
}

impl Render for Page {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.echo = None;
        let rows = painted_rows(&self.buffer, &self.lore);
        let empty = self.buffer.is_empty();
        let caret = self.caret;
        let anchor = self.anchor;
        let focus = self.focus.clone();
        let entity = cx.entity();
        let (_row, col) = location_to_point(&self.buffer, COLUMNS, caret);
        let mark = if self.buffer.is_dirty() { "●  " } else { "" };
        let status = format!(
            "{mark}Ln {}  Col {}",
            caret.line_idx.saturating_add(1),
            col.saturating_add(1)
        );
        let find_open = self.find_open;
        let find_query = self.find_query.clone();
        let mention_names = self.mentions.clone();
        let mention_pick = self.mention_pick;
        let mut height = px(0.0);
        for _ in 0..rows.len().max(1) {
            height += px(LINE_PX);
        }
        div()
            .size_full()
            .bg(rgb(DESK))
            .flex()
            .flex_col()
            .child(
                div().flex_1().flex().justify_center().child(
                    div()
                        .id("sheet")
                        .track_focus(&self.focus)
                        .on_key_down(cx.listener(Page::on_key))
                        .w(px(760.0))
                        .h_full()
                        .bg(rgb(PAPER))
                        .overflow_y_scroll()
                        .p_8()
                        .child(
                            canvas(
                                move |_bounds, _window, _cx| (rows, empty, caret, anchor),
                                move |bounds, (rows, empty, caret, anchor), window, cx| {
                                    watch_pointer(window, bounds, &rows, entity.clone());
                                    window.handle_input(
                                        &focus,
                                        ElementInputHandler::new(bounds, entity.clone()),
                                        cx,
                                    );
                                    paint_page(
                                        bounds.origin,
                                        &rows,
                                        empty,
                                        caret,
                                        anchor,
                                        window,
                                        cx,
                                    );
                                },
                            )
                            .w_full()
                            .h(height),
                        ),
                ),
            )
            .when(find_open, |sheet| {
                sheet.child(
                    div()
                        .id("find")
                        .track_focus(&self.find_focus)
                        .on_key_down(cx.listener(Page::on_find_key))
                        .h(px(32.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .text_sm()
                        .font(font("Menlo"))
                        .text_color(rgb(INK))
                        .bg(rgb(PAPER))
                        .child(SharedString::from(if find_query.is_empty() {
                            "find".to_string()
                        } else {
                            find_query
                        })),
                )
            })
            .when(!mention_names.is_empty(), |sheet| {
                sheet.child(div().flex().flex_col().px(px(16.0)).py(px(4.0)).children(
                    mention_names.into_iter().enumerate().map(|(index, name)| {
                        let color = if index == mention_pick { INK } else { QUIET };
                        div()
                            .text_sm()
                            .font(font("Menlo"))
                            .text_color(rgb(color))
                            .child(SharedString::from(name))
                    }),
                ))
            })
            .child(
                div()
                    .h(px(28.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .text_sm()
                    .font(font("Menlo"))
                    .text_color(rgb(QUIET))
                    .child(SharedString::from(status)),
            )
    }
}

fn paint_page(
    origin: Point<Pixels>,
    rows: &[PaintedRow],
    empty: bool,
    caret: Location,
    anchor: Option<Location>,
    window: &mut Window,
    cx: &mut App,
) {
    let face = font("Georgia");
    let font_size = px(FONT_PX);
    let line_height = px(LINE_PX);
    let style = PaintStyle {
        face: face.clone(),
        color: rgb(INK).into(),
        font_size,
        line_height,
    };
    if empty {
        let _hint = shape_line(
            origin,
            "start the draft here.",
            &[],
            &PaintStyle {
                face,
                color: rgb(QUIET).into(),
                font_size,
                line_height,
            },
            window,
            cx,
        );
        window.paint_quad(fill(
            Bounds {
                origin,
                size: size(px(1.0), px(LINE_PX - 6.0)),
            },
            rgb(INK),
        ));
        return;
    }
    let selection = anchor.and_then(|mark| ordered_span(mark, caret));
    let mut y = px(0.0);
    for (index, row) in rows.iter().enumerate() {
        let mut at = origin;
        at.y += y;
        let shaped = shape_line(at, &row.text, &row.marks, &style, window, cx);
        if let Some((start, end)) = selection {
            paint_wash(origin.x, at.y, row, start, end, &shaped, window);
        }
        let last_of_line = rows
            .get(index.saturating_add(1))
            .is_none_or(|next| next.line_idx != row.line_idx);
        if caret_on_row(row, caret, last_of_line) {
            let local = caret.grapheme_idx.saturating_sub(row.start);
            let byte = grapheme_byte_in_row(&row.text, local);
            let x = if row.text.is_empty() {
                px(0.0)
            } else {
                shaped.x_for_index(byte)
            };
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
}

fn caret_on_row(row: &PaintedRow, caret: Location, last_of_line: bool) -> bool {
    if row.line_idx != caret.line_idx || caret.grapheme_idx < row.start {
        return false;
    }
    caret.grapheme_idx < row.end || (caret.grapheme_idx == row.end && last_of_line)
}

fn ordered_span(a: Location, b: Location) -> Option<(Location, Location)> {
    if a.line_idx == b.line_idx && a.grapheme_idx == b.grapheme_idx {
        return None;
    }
    if a.line_idx < b.line_idx || (a.line_idx == b.line_idx && a.grapheme_idx < b.grapheme_idx) {
        Some((a, b))
    } else {
        Some((b, a))
    }
}

fn paint_wash(
    x0: Pixels,
    y: Pixels,
    row: &PaintedRow,
    start: Location,
    end: Location,
    shaped: &gpui::ShapedLine,
    window: &mut Window,
) {
    let Some((from, to)) = row_overlap(row, start, end) else {
        return;
    };
    let left = if row.text.is_empty() {
        px(0.0)
    } else {
        shaped.x_for_index(grapheme_byte_in_row(&row.text, from))
    };
    let right = if row.text.is_empty() {
        px(8.0)
    } else {
        shaped.x_for_index(grapheme_byte_in_row(&row.text, to))
    };
    let width = if right > left { right - left } else { px(8.0) };
    window.paint_quad(fill(
        Bounds {
            origin: point(x0 + left, y),
            size: size(width, px(LINE_PX - 6.0)),
        },
        rgb(WASH),
    ));
}

fn row_overlap(row: &PaintedRow, start: Location, end: Location) -> Option<(usize, usize)> {
    if row.line_idx < start.line_idx || row.line_idx > end.line_idx {
        return None;
    }
    let from = if row.line_idx == start.line_idx {
        start.grapheme_idx.max(row.start)
    } else {
        row.start
    };
    let to = if row.line_idx == end.line_idx {
        end.grapheme_idx.min(row.end)
    } else {
        row.end
    };
    if from >= to && row.start != row.end {
        return None;
    }
    Some((from.saturating_sub(row.start), to.saturating_sub(row.start)))
}

struct PaintStyle {
    face: gpui::Font,
    color: gpui::Hsla,
    font_size: Pixels,
    line_height: Pixels,
}

fn shape_line(
    origin: Point<Pixels>,
    text: &str,
    marks: &[std::ops::Range<usize>],
    style: &PaintStyle,
    window: &mut Window,
    cx: &mut App,
) -> gpui::ShapedLine {
    let shown = if text.is_empty() { " " } else { text };
    let runs = text_runs(shown, if text.is_empty() { &[] } else { marks }, style);
    let shaped = window.text_system().shape_line(
        SharedString::from(shown.to_owned()),
        style.font_size,
        &runs,
        None,
    );
    let _ = shaped.paint(origin, style.line_height, window, cx);
    shaped
}

fn text_runs(text: &str, marks: &[std::ops::Range<usize>], style: &PaintStyle) -> Vec<TextRun> {
    let plain = |len: usize, mention: bool| TextRun {
        len,
        font: style.face.clone(),
        color: style.color,
        background_color: None,
        underline: mention.then(|| gpui::UnderlineStyle {
            thickness: px(1.0),
            color: Some(style.color),
            wavy: false,
        }),
        strikethrough: None,
    };
    if marks.is_empty() {
        return vec![plain(text.len().max(1), false)];
    }
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    for mark in marks {
        let start = mark.start.min(text.len());
        let end = mark.end.min(text.len()).max(start);
        if start > cursor {
            runs.push(plain(start.saturating_sub(cursor), false));
        }
        if end > start {
            runs.push(plain(end.saturating_sub(start), true));
        }
        cursor = end;
    }
    if cursor < text.len() {
        runs.push(plain(text.len().saturating_sub(cursor), false));
    }
    if runs.is_empty() {
        runs.push(plain(text.len().max(1), false));
    }
    runs
}

fn grapheme_byte_in_row(text: &str, grapheme_idx: usize) -> usize {
    grapheme_byte(text, grapheme_idx)
}

pub(crate) fn location_at(
    window: &mut Window,
    rows: &[PaintedRow],
    origin: Point<Pixels>,
    position: Point<Pixels>,
) -> (Location, usize) {
    if rows.is_empty() {
        return (Location::default(), 0);
    }
    let row_index = row_at_y(origin.y, position.y, rows.len());
    let Some(row) = rows.get(row_index) else {
        return (Location::default(), 0);
    };
    if row.text.is_empty() {
        return (
            Location {
                grapheme_idx: row.start,
                line_idx: row.line_idx,
            },
            row_index,
        );
    }
    let style = PaintStyle {
        face: font("Georgia"),
        color: rgb(INK).into(),
        font_size: px(FONT_PX),
        line_height: px(LINE_PX),
    };
    let shaped = layout_line(&row.text, &style, window);
    let x = if position.x > origin.x {
        position.x - origin.x
    } else {
        px(0.0)
    };
    let byte = shaped.closest_index_for_x(x);
    (
        Location {
            grapheme_idx: row.start.saturating_add(grapheme_for_byte(&row.text, byte)),
            line_idx: row.line_idx,
        },
        row_index,
    )
}

fn layout_line(text: &str, style: &PaintStyle, window: &mut Window) -> gpui::ShapedLine {
    let run = TextRun {
        len: text.len(),
        font: style.face.clone(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window.text_system().shape_line(
        SharedString::from(text.to_owned()),
        style.font_size,
        &[run],
        None,
    )
}

fn grapheme_for_byte(text: &str, byte: usize) -> usize {
    let mut index = 0usize;
    loop {
        if grapheme_byte(text, index) >= byte {
            return index;
        }
        index = index.saturating_add(1);
        if index > text.len() {
            return index;
        }
    }
}

fn row_at_y(origin_y: Pixels, y: Pixels, count: usize) -> usize {
    if y <= origin_y || count == 0 {
        return 0;
    }
    let mut walked = origin_y;
    let mut row = 0usize;
    let step = px(LINE_PX);
    while row.saturating_add(1) < count && walked + step <= y {
        walked += step;
        row = row.saturating_add(1);
    }
    row
}

fn watch_pointer(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    rows: &[PaintedRow],
    entity: gpui::Entity<Page>,
) {
    let rows = rows.to_vec();
    let origin = bounds.origin;
    window.on_mouse_event({
        let rows = rows.clone();
        let entity = entity.clone();
        move |event: &gpui::MouseDownEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble || event.button != gpui::MouseButton::Left {
                return;
            }
            if !inside(bounds, event.position) {
                return;
            }
            entity.update(cx, |page, cx| {
                page.mouse_down(event, &rows, origin, window, cx);
            });
        }
    });
    window.on_mouse_event({
        let rows = rows.clone();
        let entity = entity.clone();
        move |event: &gpui::MouseMoveEvent, phase, window, cx| {
            if phase != gpui::DispatchPhase::Bubble {
                return;
            }
            entity.update(cx, |page, cx| {
                if !page.dragging && !inside(bounds, event.position) {
                    return;
                }
                page.mouse_move(&rows, origin, event.position, window, cx);
            });
        }
    });
    window.on_mouse_event({
        let entity = entity.clone();
        move |event: &gpui::MouseUpEvent, phase, _window, cx| {
            if phase != gpui::DispatchPhase::Bubble || event.button != gpui::MouseButton::Left {
                return;
            }
            entity.update(cx, |page, cx| page.mouse_up(cx));
        }
    });
}

fn inside(bounds: Bounds<Pixels>, position: Point<Pixels>) -> bool {
    position.x >= bounds.origin.x
        && position.y >= bounds.origin.y
        && position.x <= bounds.origin.x + bounds.size.width
        && position.y <= bounds.origin.y + bounds.size.height
}

fn painted_rows(buffer: &Buffer, lore: &yonro_core::LoreBook) -> Vec<PaintedRow> {
    visual_rows(buffer, COLUMNS)
        .into_iter()
        .map(|row| {
            let text = row_text(buffer, &row);
            let marks = mention_marks(buffer, lore, &row, &text);
            PaintedRow {
                text,
                line_idx: row.line_idx,
                start: row.start,
                end: row.end,
                marks,
            }
        })
        .collect()
}

fn mention_marks(
    buffer: &Buffer,
    lore: &yonro_core::LoreBook,
    row: &yonro_core::VisualRow,
    row_text: &str,
) -> Vec<std::ops::Range<usize>> {
    let Some(line) = buffer.get_line(row.line_idx) else {
        return Vec::new();
    };
    let mut marks = Vec::new();
    for mention in lore.parse_mentions(&line) {
        let start = grapheme_for_byte(&line, mention.byte_range.start);
        let end = grapheme_for_byte(&line, mention.byte_range.end);
        if end <= row.start || start >= row.end {
            continue;
        }
        let local_start = start.max(row.start).saturating_sub(row.start);
        let local_end = end.min(row.end).saturating_sub(row.start);
        let byte_start = grapheme_byte(row_text, local_start);
        let byte_end = grapheme_byte(row_text, local_end);
        if byte_end > byte_start {
            marks.push(byte_start..byte_end);
        }
    }
    marks
}

fn lore_for(path: Option<&std::path::Path>) -> yonro_core::LoreBook {
    let Some(path) = path else {
        return yonro_core::LoreBook::default();
    };
    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default()
    };
    if root.as_os_str().is_empty() {
        return yonro_core::LoreBook::default();
    }
    yonro_core::Project::load(&root).lore
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
