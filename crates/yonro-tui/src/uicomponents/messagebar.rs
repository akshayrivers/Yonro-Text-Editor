use super::UIComponent;
use crate::prelude::*;
use crate::terminal::Terminal;
use std::{
    io::Error,
    time::{Duration, Instant},
};
const DEFAULT_DURATION: Duration = Duration::new(5, 0);
struct Message {
    text: String,
    time: Instant,
}
impl Default for Message {
    fn default() -> Self {
        Self {
            text: String::new(),
            time: Instant::now(),
        }
    }
}
impl Message {
    fn is_expired(&self) -> bool {
        Instant::now().duration_since(self.time) > DEFAULT_DURATION
    }
}
#[derive(Default)]
pub struct MessageBar {
    current_msg: Message,
    needs_redraw: bool,
    cleared_after_expiry: bool,
    rect: Rect,
}

impl MessageBar {
    pub fn update_message(&mut self, new_message: &str) {
        self.current_msg = Message {
            text: new_message.to_string(),
            time: Instant::now(),
        };
        self.cleared_after_expiry = false;
        self.mark_redraw(true);
    }
}

impl UIComponent for MessageBar {
    fn mark_redraw(&mut self, value: bool) {
        self.needs_redraw = value;
    }
    fn needs_redraw(&self) -> bool {
        (!self.cleared_after_expiry && self.current_msg.is_expired()) || self.needs_redraw
    }
    fn rect(&self) -> Rect {
        self.rect
    }

    fn set_size(&mut self, rect: Rect) {
        self.rect = rect;
    }

    fn draw(&mut self) -> Result<(), Error> {
        if self.current_msg.is_expired() {
            self.cleared_after_expiry = true;
            // upon expiry we need to clear the msg first
        }
        let message = if self.current_msg.is_expired() {
            ""
        } else {
            &self.current_msg.text
        };

        Terminal::print_rect(self.rect, 0, message)
    }
}
