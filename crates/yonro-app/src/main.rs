//! Spike: a warm-paper window and one serif line.
//!
//! No buffer yet. This only proves GPUI builds and paints on this machine.

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, Window, WindowBounds,
    WindowOptions,
};

/// Warm paper, dark default. Named here until a theme module exists.
const PAPER: u32 = 0x2c26_1fff;
const INK: u32 = 0xf3e6_d0ff;

struct Page;

impl Render for Page {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .bg(rgb(PAPER))
            .size_full()
            .justify_center()
            .items_center()
            .text_3xl()
            .font_family("Georgia")
            .text_color(rgb(INK))
            .child("once upon a time")
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.0), px(720.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_window, cx| cx.new(|_cx| Page),
        )
        .ok();
    });
}
