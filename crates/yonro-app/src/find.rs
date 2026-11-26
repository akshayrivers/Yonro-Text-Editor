//! Find next and previous, and the `@` name list.
//!
//! Hits and names come from core. This view only moves the caret.

use gpui::{Context, KeyDownEvent, Window};
use yonro_core::{is_mention_char, prelude::Location, Line};

use crate::document::Page;

impl Page {
    pub(crate) fn on_find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mods = &event.keystroke.modifiers;
        if mods.platform || mods.control {
            match event.keystroke.key.as_str() {
                "enter" if mods.shift => self.replace_every(),
                "enter" => self.replace_one(),
                _ => {}
            }
            cx.notify();
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => {
                self.find_open = false;
                self.replace_on = false;
                self.focus.focus(window);
            }
            "tab" => self.replace_on = !self.replace_on,
            "enter" if mods.shift => self.seek(self.anchor.unwrap_or(self.caret), false),
            "enter" => self.seek(self.caret, true),
            "backspace" => {
                let query = if self.replace_on {
                    &mut self.replace_query
                } else {
                    &mut self.find_query
                };
                let mut chars: Vec<char> = query.chars().collect();
                chars.pop();
                *query = chars.into_iter().collect();
                if !self.replace_on {
                    self.seek(self.find_from, true);
                }
            }
            _ => {
                if let Some(text) = event.keystroke.key_char.as_deref() {
                    if !text.is_empty() && text.chars().all(|ch| !ch.is_control()) {
                        if self.replace_on {
                            self.replace_query.push_str(text);
                        } else {
                            self.find_query.push_str(text);
                            self.seek(self.find_from, true);
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn open_find(&mut self, window: &mut Window) {
        self.find_open = true;
        self.find_from = self.caret;
        self.replace_on = false;
        self.find_focus.focus(window);
    }

    fn replace_one(&mut self) {
        let Some(caret) = self.log.replace_next(
            &mut self.buffer,
            self.anchor.unwrap_or(self.caret),
            &self.find_query,
            &self.replace_query,
        ) else {
            return;
        };
        self.caret = caret;
        self.anchor = None;
    }

    fn replace_every(&mut self) {
        let count = self
            .log
            .replace_all(&mut self.buffer, &self.find_query, &self.replace_query);
        if count == 0 {
            return;
        }
        self.anchor = None;
    }

    pub(crate) fn mention_key(&mut self, event: &KeyDownEvent) -> bool {
        if self.mentions.is_empty() {
            return false;
        }
        match event.keystroke.key.as_str() {
            "enter" => self.accept_mention(),
            "escape" => self.mentions.clear(),
            "up" => {
                self.mention_pick = self.mention_pick.saturating_sub(1);
            }
            "down" => {
                let last = self.mentions.len().saturating_sub(1);
                if self.mention_pick < last {
                    self.mention_pick = self.mention_pick.saturating_add(1);
                }
            }
            _ => return false,
        }
        true
    }

    pub(crate) fn refresh_mentions(&mut self) {
        if self.verse {
            self.mentions.clear();
            self.mention_at = None;
            return;
        }
        let Some((at, prefix)) = self.mention_token() else {
            self.mentions.clear();
            self.mention_at = None;
            return;
        };
        self.mention_at = Some(at);
        self.mentions = yonro_core::api::lore_search(&self.lore, &prefix, 8)
            .into_iter()
            .map(|hit| hit.name)
            .collect();
        if self.mention_pick >= self.mentions.len() {
            self.mention_pick = 0;
        }
    }

    fn seek(&mut self, from: Location, forward: bool) {
        if self.find_query.is_empty() {
            return;
        }
        let hit = if forward {
            self.buffer.search_forward(&self.find_query, from)
        } else {
            self.buffer.search_backward(&self.find_query, from)
        };
        let Some(hit) = hit else {
            return;
        };
        let len = Line::from(self.find_query.as_str()).grapheme_count();
        self.anchor = Some(hit);
        self.caret = Location {
            grapheme_idx: hit.grapheme_idx.saturating_add(len),
            line_idx: hit.line_idx,
        };
    }

    fn accept_mention(&mut self) {
        let Some(name) = self.mentions.get(self.mention_pick).cloned() else {
            return;
        };
        let Some(start) = self.mention_at else {
            return;
        };
        let now = std::time::Instant::now();
        self.caret = self
            .log
            .delete_range(&mut self.buffer, start, self.caret, now);
        let text = format!("@{name}");
        self.caret = self
            .log
            .insert_text(&mut self.buffer, self.caret, &text, now);
        self.anchor = None;
        self.mentions.clear();
        self.mention_at = None;
    }

    fn mention_token(&self) -> Option<(Location, String)> {
        let mut grapheme_idx = self.caret.grapheme_idx;
        let line_idx = self.caret.line_idx;
        let mut chars = Vec::new();
        while grapheme_idx > 0 {
            let prev = grapheme_idx.saturating_sub(1);
            let at = Location {
                grapheme_idx: prev,
                line_idx,
            };
            let ch = self.buffer.get_char_at(at)?;
            if ch == '@' {
                chars.reverse();
                let prefix: String = chars.into_iter().collect();
                return Some((at, prefix));
            }
            if !is_mention_char(ch) {
                return None;
            }
            chars.push(ch);
            grapheme_idx = prev;
        }
        None
    }
}
