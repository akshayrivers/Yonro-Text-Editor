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

use crate::theme::{ACCENT, DESK, INK, MARK, PAPER, QUIET, WASH};

pub(crate) const COLUMNS: usize = 70;
const FONT_PX: f32 = 20.0;
const LINE_PX: f32 = 34.0;

#[derive(Clone, Default)]
pub(crate) struct InkSpan {
    pub(crate) range: std::ops::Range<usize>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) strike: bool,
    pub(crate) highlight: bool,
    pub(crate) accent: bool,
    pub(crate) wavy: bool,
}

#[derive(Clone)]
pub(crate) struct PaintedRow {
    pub(crate) text: String,
    pub(crate) line_idx: usize,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) spans: Vec<InkSpan>,
    pub(crate) face: String,
    pub(crate) font_px: f32,
    pub(crate) line_px: f32,
    pub(crate) align: yonro_core::Align,
    pub(crate) indent: u8,
    pub(crate) hang: bool,
    pub(crate) marker: String,
    pub(crate) zoom: f32,
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
    pub(crate) sticky: Vec<yonro_core::MarkKind>,
    pub(crate) painter: Vec<yonro_core::MarkKind>,
    pub(crate) note_open: bool,
    pub(crate) note_draft: String,
    pub(crate) note_focus: FocusHandle,
    pub(crate) lore: yonro_core::LoreBook,
    pub(crate) find_open: bool,
    pub(crate) find_query: String,
    pub(crate) replace_query: String,
    pub(crate) replace_on: bool,
    pub(crate) zoom: f32,
    pub(crate) find_from: Location,
    pub(crate) find_focus: FocusHandle,
    pub(crate) mentions: Vec<String>,
    pub(crate) mention_at: Option<Location>,
    pub(crate) mention_pick: usize,
    pub(crate) zen: bool,
    pub(crate) scroll: gpui::ScrollHandle,
    pub(crate) title: String,
    pub(crate) scenes: Vec<String>,
    pub(crate) creating: bool,
    pub(crate) profile: yonro_core::Profile,
}

impl Page {
    /// Open a scene file, or an empty page when `path` is missing.
    pub fn open(path: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let lore = lore_for(path.as_deref());
        let (buffer, path) = load(path);
        let (title, scenes) = chrome_for(path.as_deref());
        let (creating, profile) = desk_for(path.as_deref());
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
            sticky: Vec::new(),
            painter: Vec::new(),
            note_open: false,
            note_draft: String::new(),
            note_focus: cx.focus_handle(),
            lore,
            find_open: false,
            find_query: String::new(),
            replace_query: String::new(),
            replace_on: false,
            zoom: 1.0,
            find_from: Location::default(),
            find_focus: cx.focus_handle(),
            mentions: Vec::new(),
            mention_at: None,
            mention_pick: 0,
            zen: false,
            scroll: gpui::ScrollHandle::new(),
            title,
            scenes,
            creating,
            profile,
        }
    }

    pub(crate) fn choose_profile(&mut self, profile: yonro_core::Profile) {
        self.profile = profile;
        self.creating = false;
        let Some(root) = workspace_root(self.path.as_deref()) else {
            return;
        };
        if !yonro_core::Project::has_project(&root) {
            let _ = yonro_core::Project::create(&root, "untitled");
        }
        let project = yonro_core::Project::load(&root);
        let _ = project.set_profile(profile);
        self.title = project.manuscript.title().to_string();
        self.scenes.clear();
        scene_titles(
            &project.manuscript,
            project.manuscript.root(),
            &mut self.scenes,
        );
    }
}

impl Render for Page {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.echo = None;
        let rows = painted_rows(&self.buffer, &self.lore, self.log.format(), self.zoom);
        if self.zen {
            settle_caret(&self.scroll, window, &rows, self.caret);
        }
        let empty = self.buffer.is_empty();
        let caret = self.caret;
        let anchor = self.anchor;
        let focus = self.focus.clone();
        let entity = cx.entity();
        let (_row, col) = location_to_point(&self.buffer, COLUMNS, caret);
        let mark = if self.buffer.is_dirty() { "●  " } else { "" };
        let percent = (self.zoom * 100.0).round();
        let zoomed = if (percent - 100.0).abs() > 0.5 {
            format!("{percent:.0}%  ")
        } else {
            String::new()
        };
        let (_, words, _) = self.buffer.word_count_stats();
        let status = format!(
            "{mark}{zoomed}{words} words  Ln {}  Col {}",
            caret.line_idx.saturating_add(1),
            col.saturating_add(1)
        );
        let find_open = self.find_open;
        let find_query = self.find_query.clone();
        let replace_query = self.replace_query.clone();
        let replace_on = self.replace_on;
        let note_label = self
            .log
            .format()
            .notes()
            .first()
            .map(|note| note.text.clone());
        let note_open = self.note_open;
        let note_draft = self.note_draft.clone();
        let mention_names = self.mentions.clone();
        let mention_pick = self.mention_pick;
        let mut height = px(0.0);
        if rows.is_empty() {
            height = px(LINE_PX);
        }
        for row in &rows {
            height += px(row.line_px);
        }
        div()
            .size_full()
            .bg(rgb(DESK))
            .flex()
            .flex_col()
            .when(!self.zen, |column| {
                let title = self.title.clone();
                column.child(
                    div()
                        .h(px(28.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_sm()
                        .font(font("Menlo"))
                        .text_color(rgb(QUIET))
                        .child("yonro")
                        .child(SharedString::from(title))
                        .child("zen"),
                )
            })
            .when(self.creating, |column| {
                column.child(
                    div()
                        .h(px(28.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .text_sm()
                        .font(font("Menlo"))
                        .text_color(rgb(INK))
                        .child("novel   story   pieces"),
                )
            })
            .child(
                div()
                    .flex_1()
                    .flex()
                    .when(!self.zen, |row| row.child(binder_column(&self.scenes)))
                    .justify_center()
                    .child(
                        div()
                            .id("sheet")
                            .track_focus(&self.focus)
                            .track_scroll(&self.scroll)
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
                    )
                    .when(note_label.is_some(), |row| {
                        let text = note_label.clone().unwrap_or_default();
                        row.child(
                            div()
                                .w(px(200.0))
                                .p_4()
                                .text_sm()
                                .font(font("Menlo"))
                                .text_color(rgb(QUIET))
                                .child(SharedString::from(text)),
                        )
                    }),
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
                        .child(SharedString::from(find_bar(
                            &find_query,
                            &replace_query,
                            replace_on,
                        ))),
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
            .when(note_open, |sheet| {
                let draft = if note_draft.is_empty() {
                    "note".to_string()
                } else {
                    note_draft
                };
                sheet.child(
                    div()
                        .id("note")
                        .track_focus(&self.note_focus)
                        .on_key_down(cx.listener(Page::on_note_key))
                        .h(px(32.0))
                        .px(px(16.0))
                        .flex()
                        .items_center()
                        .text_sm()
                        .font(font("Menlo"))
                        .text_color(rgb(INK))
                        .bg(rgb(PAPER))
                        .child(SharedString::from(draft)),
                )
            })
            .child(
                div()
                    .h(px(28.0))
                    .px(px(16.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .font(font("Menlo"))
                    .child(strip_label(
                        self.style_label(),
                        self.style_label() != "body",
                    ))
                    .child(strip_label(
                        self.align_label(),
                        self.align_label() != "left",
                    ))
                    .child(strip_label(self.list_label(), self.list_label() != "list"))
                    .child(strip_label("b", self.strip_on(&yonro_core::MarkKind::Bold)))
                    .child(strip_label(
                        "i",
                        self.strip_on(&yonro_core::MarkKind::Italic),
                    ))
                    .child(strip_label(
                        "u",
                        self.strip_on(&yonro_core::MarkKind::Underline),
                    ))
                    .child(strip_label(
                        "s",
                        self.strip_on(&yonro_core::MarkKind::Strike),
                    ))
                    .child(strip_label(
                        "hl",
                        self.strip_on(&yonro_core::MarkKind::Highlight { swatch: 0 }),
                    ))
                    .child(strip_label(
                        "ink",
                        self.strip_on(&yonro_core::MarkKind::Color { swatch: 0 }),
                    ))
                    .child(strip_label(self.face_label(), true))
                    .child(strip_label(self.spacing_label(), true))
                    .child(strip_label("paint", !self.painter.is_empty())),
            )
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

fn strip_label(label: &'static str, on: bool) -> gpui::Div {
    let color = if on { INK } else { QUIET };
    div().text_color(rgb(color)).child(label)
}

fn find_bar(query: &str, replacement: &str, replace_on: bool) -> String {
    let query = if query.is_empty() { "find" } else { query };
    let replacement = if replacement.is_empty() {
        "replace"
    } else {
        replacement
    };
    if replace_on {
        format!("{query}   /   {replacement}  ·")
    } else {
        format!("{query}  ·   /   {replacement}")
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
    if empty {
        let face = font("Georgia");
        let font_size = px(FONT_PX);
        let line_height = px(LINE_PX);
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
        let style = row_style(row);
        let probe = layout_line(
            if row.text.is_empty() { " " } else { &row.text },
            &style,
            window,
        );
        let dx = text_dx(row, probe.width);
        let mut at = origin;
        at.y += y;
        at.x += dx;
        if !row.marker.is_empty() {
            let mut mark_at = origin;
            mark_at.y += y;
            mark_at.x += px(f32::from(row.indent) * 24.0 * row.zoom);
            let _marker = shape_line(mark_at, &row.marker, &[], &style, window, cx);
        }
        let shaped = shape_line(at, &row.text, &row.spans, &style, window, cx);
        if let Some((start, end)) = selection {
            paint_wash(at.x, at.y, row, start, end, &shaped, window);
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
                    origin: point(at.x + x, at.y),
                    size: size(px(1.0), px(row.line_px - 6.0)),
                },
                rgb(INK),
            ));
        }
        y += px(row.line_px);
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
            size: size(width, px(row.line_px - 6.0)),
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
    spans: &[InkSpan],
    style: &PaintStyle,
    window: &mut Window,
    cx: &mut App,
) -> gpui::ShapedLine {
    let shown = if text.is_empty() { " " } else { text };
    let runs = text_runs(shown, if text.is_empty() { &[] } else { spans }, style);
    let shaped = window.text_system().shape_line(
        SharedString::from(shown.to_owned()),
        style.font_size,
        &runs,
        None,
    );
    let _ = shaped.paint(origin, style.line_height, window, cx);
    shaped
}

fn text_runs(text: &str, spans: &[InkSpan], style: &PaintStyle) -> Vec<TextRun> {
    if spans.is_empty() {
        return vec![plain_run(text.len().max(1), style, &InkSpan::default())];
    }
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    for span in spans {
        let start = span.range.start.min(text.len());
        let end = span.range.end.min(text.len()).max(start);
        if start > cursor {
            runs.push(plain_run(
                start.saturating_sub(cursor),
                style,
                &InkSpan::default(),
            ));
        }
        if end > start {
            runs.push(plain_run(end.saturating_sub(start), style, span));
        }
        cursor = end;
    }
    if cursor < text.len() {
        runs.push(plain_run(
            text.len().saturating_sub(cursor),
            style,
            &InkSpan::default(),
        ));
    }
    if runs.is_empty() {
        runs.push(plain_run(text.len().max(1), style, &InkSpan::default()));
    }
    runs
}

fn plain_run(len: usize, style: &PaintStyle, span: &InkSpan) -> TextRun {
    let mut face = style.face.clone();
    if span.bold {
        face = face.bold();
    }
    if span.italic {
        face = face.italic();
    }
    let color = if span.accent {
        rgb(ACCENT).into()
    } else {
        style.color
    };
    TextRun {
        len,
        font: face,
        color,
        background_color: span.highlight.then(|| rgb(MARK).into()),
        underline: (span.underline || span.wavy).then(|| gpui::UnderlineStyle {
            thickness: px(1.0),
            color: Some(color),
            wavy: span.wavy,
        }),
        strikethrough: span.strike.then(|| gpui::StrikethroughStyle {
            thickness: px(1.0),
            color: Some(color),
        }),
    }
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
    let row_index = row_at_y(origin.y, position.y, rows);
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
    let style = row_style(row);
    let shaped = layout_line(&row.text, &style, window);
    let dx = text_dx(row, shaped.width);
    let x = if position.x > origin.x + dx {
        position.x - origin.x - dx
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

fn row_at_y(origin_y: Pixels, y: Pixels, rows: &[PaintedRow]) -> usize {
    if y <= origin_y || rows.is_empty() {
        return 0;
    }
    let mut walked = origin_y;
    let mut row = 0usize;
    while row.saturating_add(1) < rows.len() {
        let step = px(rows[row].line_px);
        if walked + step > y {
            break;
        }
        walked += step;
        row = row.saturating_add(1);
    }
    row
}

fn row_style(row: &PaintedRow) -> PaintStyle {
    PaintStyle {
        face: font(SharedString::from(row.face.clone())),
        color: rgb(INK).into(),
        font_size: px(row.font_px),
        line_height: px(row.line_px),
    }
}

fn text_dx(row: &PaintedRow, text_width: Pixels) -> Pixels {
    let left = px((f32::from(row.indent) * 24.0 + if row.hang { 24.0 } else { 0.0 }) * row.zoom);
    let sheet = px(700.0 * row.zoom);
    let room = if sheet > left { sheet - left } else { px(0.0) };
    let extra = if room > text_width {
        room - text_width
    } else {
        px(0.0)
    };
    match row.align {
        yonro_core::Align::Center => left + extra * 0.5,
        yonro_core::Align::Right => left + extra,
        yonro_core::Align::Left | yonro_core::Align::Justify => left,
    }
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

fn painted_rows(
    buffer: &Buffer,
    lore: &yonro_core::LoreBook,
    format: &yonro_core::Format,
    zoom: f32,
) -> Vec<PaintedRow> {
    let mut flat = 0usize;
    let mut previous = None;
    visual_rows(buffer, COLUMNS)
        .into_iter()
        .map(|row| {
            let first = previous != Some(row.line_idx);
            if previous.is_some_and(|line: usize| line != row.line_idx) {
                flat = flat.saturating_add(1);
            }
            previous = Some(row.line_idx);
            let text = row_text(buffer, &row);
            let mentions = mention_marks(buffer, lore, &row, &text);
            let spans = ink_spans(&text, flat, format, &mentions);
            let row_len = row.end.saturating_sub(row.start);
            flat = flat.saturating_add(row_len);
            let para = crate::strip::paragraph_on(format, row.line_idx);
            let (font_px, line_px) = line_metrics(para.style, format.spacing());
            let zoom = zoom.clamp(0.8, 2.0);
            let hang = para.list != yonro_core::ListKind::None;
            let marker = if first {
                list_marker(format, row.line_idx, para.list)
            } else {
                String::new()
            };
            let face = if format.face().is_empty() {
                "Georgia".to_string()
            } else {
                format.face().to_string()
            };
            PaintedRow {
                text,
                line_idx: row.line_idx,
                start: row.start,
                end: row.end,
                spans,
                face,
                font_px: font_px * zoom,
                line_px: line_px * zoom,
                align: para.align,
                indent: para.indent,
                hang,
                marker,
                zoom,
            }
        })
        .collect()
}

fn line_metrics(style: yonro_core::ParaStyle, spacing: yonro_core::Spacing) -> (f32, f32) {
    let (font_px, extra) = match style {
        yonro_core::ParaStyle::Normal => (20.0, 0.0),
        yonro_core::ParaStyle::H1 => (30.0, 10.0),
        yonro_core::ParaStyle::H2 => (26.0, 6.0),
        yonro_core::ParaStyle::H3 => (22.0, 2.0),
    };
    let base = match spacing {
        yonro_core::Spacing::Compact => 28.0,
        yonro_core::Spacing::Roomy => 34.0,
        yonro_core::Spacing::Airy => 42.0,
    };
    (font_px, base + extra)
}

fn list_marker(format: &yonro_core::Format, line: usize, kind: yonro_core::ListKind) -> String {
    match kind {
        yonro_core::ListKind::None => String::new(),
        yonro_core::ListKind::Bullet => "•".to_string(),
        yonro_core::ListKind::Number => format!("{}.", list_number(format, line)),
    }
}

fn list_number(format: &yonro_core::Format, line: usize) -> u32 {
    let mut count = 0u32;
    let mut cursor = line;
    loop {
        if crate::strip::paragraph_on(format, cursor).list != yonro_core::ListKind::Number {
            break;
        }
        count = count.saturating_add(1);
        if cursor == 0 {
            break;
        }
        cursor = cursor.saturating_sub(1);
    }
    count
}

fn ink_spans(
    text: &str,
    row_flat: usize,
    format: &yonro_core::Format,
    mentions: &[std::ops::Range<usize>],
) -> Vec<InkSpan> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    let mut grapheme = 0usize;
    let mut start_byte = 0usize;
    let typos = yonro_core::misspelled_bytes(text);
    let mut current = look(format, row_flat, mentions, &typos, 0);
    loop {
        let byte = grapheme_byte(text, grapheme);
        if byte >= text.len() {
            break;
        }
        let flags = look(
            format,
            row_flat.saturating_add(grapheme),
            mentions,
            &typos,
            byte,
        );
        if grapheme > 0 && !same_look(&flags, &current) {
            spans.push(InkSpan {
                range: start_byte..byte,
                ..current.clone()
            });
            start_byte = byte;
            current = flags;
        } else if grapheme == 0 {
            current = flags;
        }
        grapheme = grapheme.saturating_add(1);
    }
    let end_byte = text.len();
    if end_byte > start_byte {
        spans.push(InkSpan {
            range: start_byte..end_byte,
            ..current
        });
    }
    spans
}

fn look(
    format: &yonro_core::Format,
    flat: usize,
    mentions: &[std::ops::Range<usize>],
    typos: &[(usize, usize)],
    byte: usize,
) -> InkSpan {
    let mut span = InkSpan {
        underline: mentions
            .iter()
            .any(|mention| byte >= mention.start && byte < mention.end),
        wavy: typos.iter().any(|typo| byte >= typo.0 && byte < typo.1),
        ..InkSpan::default()
    };
    for mark in format.marks() {
        if flat < mark.start || flat >= mark.end {
            continue;
        }
        match mark.kind {
            yonro_core::MarkKind::Bold => span.bold = true,
            yonro_core::MarkKind::Italic => span.italic = true,
            yonro_core::MarkKind::Underline => span.underline = true,
            yonro_core::MarkKind::Strike => span.strike = true,
            yonro_core::MarkKind::Highlight { .. } => span.highlight = true,
            yonro_core::MarkKind::Color { .. } => span.accent = true,
            yonro_core::MarkKind::Size { .. } | yonro_core::MarkKind::Link { .. } => {}
        }
    }
    span
}

fn same_look(left: &InkSpan, right: &InkSpan) -> bool {
    left.bold == right.bold
        && left.italic == right.italic
        && left.underline == right.underline
        && left.strike == right.strike
        && left.highlight == right.highlight
        && left.accent == right.accent
        && left.wavy == right.wavy
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

fn binder_column(scenes: &[String]) -> gpui::Div {
    let lines: Vec<String> = if scenes.is_empty() {
        vec!["no scenes yet. press a to add an act, then c, then s.".to_string()]
    } else {
        scenes.to_vec()
    };
    div()
        .w(px(200.0))
        .h_full()
        .p_4()
        .flex()
        .flex_col()
        .gap_1()
        .text_sm()
        .font(font("Menlo"))
        .text_color(rgb(QUIET))
        .children(
            lines
                .into_iter()
                .map(|title| div().child(SharedString::from(title))),
        )
}

fn settle_caret(
    scroll: &gpui::ScrollHandle,
    window: &Window,
    rows: &[PaintedRow],
    caret: Location,
) {
    let mut above = 0.0f32;
    for row in rows {
        let on_row = row.line_idx == caret.line_idx
            && caret.grapheme_idx >= row.start
            && caret.grapheme_idx <= row.end;
        if on_row {
            break;
        }
        above += row.line_px;
    }
    let view = window.viewport_size().height / px(1.0);
    let target = above - view * 0.45;
    let y = if target > 0.0 { target } else { 0.0 };
    scroll.set_offset(point(px(0.0), px(y)));
}

fn desk_for(path: Option<&std::path::Path>) -> (bool, yonro_core::Profile) {
    let Some(root) = workspace_root(path) else {
        return (true, yonro_core::Profile::Novel);
    };
    if yonro_core::Project::has_project(&root) {
        (false, yonro_core::Project::load(&root).profile())
    } else {
        (true, yonro_core::Profile::Novel)
    }
}

fn workspace_root(path: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let path = path?;
    if path.is_dir() {
        return Some(path.to_path_buf());
    }
    path.parent().map(std::path::Path::to_path_buf)
}

fn chrome_for(path: Option<&std::path::Path>) -> (String, Vec<String>) {
    let Some(path) = path else {
        return ("untitled".to_string(), Vec::new());
    };
    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_default()
    };
    if root.as_os_str().is_empty() || !yonro_core::Project::has_project(&root) {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("untitled");
        return (name.to_string(), vec![name.to_string()]);
    }
    let project = yonro_core::Project::load(&root);
    let title = project.manuscript.title().to_string();
    let mut scenes = Vec::new();
    scene_titles(&project.manuscript, project.manuscript.root(), &mut scenes);
    (title, scenes)
}

fn scene_titles(
    manuscript: &yonro_core::Manuscript,
    id: yonro_core::NodeId,
    out: &mut Vec<String>,
) {
    for child in manuscript.children(id) {
        if !child.alive {
            continue;
        }
        if child.kind == yonro_core::NodeKind::Scene {
            out.push(child.title.clone());
        } else {
            scene_titles(manuscript, child.id, out);
        }
    }
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
