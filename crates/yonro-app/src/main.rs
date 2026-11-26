//! Yonro desktop page.
//!
//! GPUI is the window and the shaper. `yonro-core` owns the rope.

mod document;
mod keys;
mod theme;

use std::path::PathBuf;

use document::Page;
use gpui::prelude::*;
use gpui::{px, size, App, Application, Bounds, WindowBounds, WindowOptions};

fn main() {
    Application::new().run(|cx: &mut App| {
        let path = std::env::args().nth(1).map(PathBuf::from);
        let bounds = Bounds::centered(None, size(px(960.0), px(720.0)), cx);
        let opened = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let page = cx.new(|cx| Page::open(path, cx));
                page.update(cx, |page, _cx| page.focus.focus(window));
                page
            },
        );
        if opened.is_err() {
            eprintln!("yonro: could not open the window");
        }
    });
}
