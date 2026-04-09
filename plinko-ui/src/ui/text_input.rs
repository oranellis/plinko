//! A simple single-line text input component with cursor navigation.

use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

/// Single-line text input with a byte-index cursor.
pub struct TextInput {
    pub content: String,
    /// Byte index into `content`. Always on a valid UTF-8 char boundary.
    pub cursor: usize,
    pub focused: bool,
    /// Horizontal scroll offset in pixels.  Interior-mutable so the render
    /// function can update it without requiring `&mut self` on the containing
    /// struct (render methods take `&self`).
    pub scroll_x: std::cell::Cell<f32>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TextInput {
    /// Create a new `TextInput` with the given initial content.
    /// The cursor is placed at the end.
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let cursor = content.len();
        Self {
            content,
            cursor,
            focused: false,
            scroll_x: std::cell::Cell::new(0.0),
        }
    }

    /// Replace the content and reset the cursor to the end.
    pub fn set_content(&mut self, s: impl Into<String>) {
        self.content = s.into();
        self.cursor = self.content.len();
        self.scroll_x.set(0.0);
    }

    /// Insert a string at the cursor position and advance the cursor past it.
    pub fn insert_str(&mut self, s: &str) {
        let cursor = self.clamped_cursor();
        self.content.insert_str(cursor, s);
        self.cursor = cursor + s.len();
    }

    /// Delete the character immediately before the cursor (backspace).
    /// Does nothing if the cursor is at position 0.
    pub fn backspace(&mut self) {
        let cursor = self.clamped_cursor();
        if cursor == 0 {
            return;
        }
        // Walk back to find the previous char boundary.
        let prev = self.prev_char_boundary(cursor);
        self.content.drain(prev..cursor);
        self.cursor = prev;
    }

    /// Move the cursor one character to the left.
    pub fn move_left(&mut self) {
        let cursor = self.clamped_cursor();
        self.cursor = self.prev_char_boundary(cursor);
    }

    /// Move the cursor one character to the right.
    pub fn move_right(&mut self) {
        let cursor = self.clamped_cursor();
        if cursor >= self.content.len() {
            return;
        }
        // Walk forward to find the next char boundary.
        let mut next = cursor + 1;
        while next < self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        self.cursor = next;
    }

    /// Move the cursor to the beginning of the content.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the content.
    pub fn move_end(&mut self) {
        self.cursor = self.content.len();
    }

    /// Delete the character immediately after the cursor (forward delete / Delete key).
    /// Does nothing if the cursor is at the end.
    pub fn delete_forward(&mut self) {
        let cursor = self.clamped_cursor();
        if cursor >= self.content.len() {
            return;
        }
        let mut next = cursor + 1;
        while next < self.content.len() && !self.content.is_char_boundary(next) {
            next += 1;
        }
        self.content.drain(cursor..next);
    }

    /// Move the cursor one word to the left (Ctrl+Left).
    pub fn move_word_left(&mut self) {
        let mut pos = self.clamped_cursor();
        // Skip whitespace before the cursor.
        while pos > 0 {
            let prev = self.prev_char_boundary(pos);
            if self.content[prev..pos]
                .chars()
                .next()
                .map(|c| c.is_whitespace())
                .unwrap_or(false)
            {
                pos = prev;
            } else {
                break;
            }
        }
        // Skip the word characters.
        while pos > 0 {
            let prev = self.prev_char_boundary(pos);
            if self.content[prev..pos]
                .chars()
                .next()
                .map(|c| !c.is_whitespace())
                .unwrap_or(false)
            {
                pos = prev;
            } else {
                break;
            }
        }
        self.cursor = pos;
    }

    /// Move the cursor one word to the right (Ctrl+Right).
    pub fn move_word_right(&mut self) {
        let mut pos = self.clamped_cursor();
        let len = self.content.len();
        // Skip the current word characters.
        while pos < len {
            let ch = self.content[pos..].chars().next().unwrap();
            if !ch.is_whitespace() {
                pos += ch.len_utf8();
            } else {
                break;
            }
        }
        // Skip whitespace after the word.
        while pos < len {
            let ch = self.content[pos..].chars().next().unwrap();
            if ch.is_whitespace() {
                pos += ch.len_utf8();
            } else {
                break;
            }
        }
        self.cursor = pos;
    }

    /// Handle a keyboard key for standard single-line editing.
    ///
    /// Returns `true` if the key was consumed (and the caller should redraw).
    /// Returns `false` for keys the caller must handle (Tab, Enter, Escape, …).
    pub fn handle_key(&mut self, key: &Key, modifiers: &Modifiers) -> bool {
        match key {
            Key::Named(NamedKey::Backspace) => {
                self.backspace();
                true
            }
            Key::Named(NamedKey::Delete) => {
                self.delete_forward();
                true
            }
            Key::Named(NamedKey::ArrowLeft) => {
                if modifiers.state().control_key() {
                    self.move_word_left();
                } else {
                    self.move_left();
                }
                true
            }
            Key::Named(NamedKey::ArrowRight) => {
                if modifiers.state().control_key() {
                    self.move_word_right();
                } else {
                    self.move_right();
                }
                true
            }
            Key::Named(NamedKey::Home) => {
                self.move_home();
                true
            }
            Key::Named(NamedKey::End) => {
                self.move_end();
                true
            }
            Key::Named(NamedKey::Space) => {
                self.insert_str(" ");
                true
            }
            Key::Character(s) if s.chars().all(|c| !c.is_control()) => {
                self.insert_str(s.as_str());
                true
            }
            _ => false,
        }
    }

    /// Paste text into the input at the cursor, replacing newlines with spaces.
    pub fn handle_paste(&mut self, text: &str) {
        let filtered: String = text
            .chars()
            .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
            .collect();
        self.insert_str(&filtered);
    }

    /// Return the byte cursor position closest to `x_in_inner` pixels from the
    /// left edge of the inner text area (already offset by `scroll_x`).
    pub fn cursor_for_x(&self, x_in_inner: f32, font: &skia_safe::Font) -> usize {
        let mut best_pos = 0usize;
        let mut best_dist = x_in_inner.abs(); // distance to position 0

        for (i, _) in self.content.char_indices() {
            let adv = font.measure_str(&self.content[..i], None).0;
            let dist = (adv - x_in_inner).abs();
            if dist < best_dist {
                best_dist = dist;
                best_pos = i;
            }
        }
        // Also check the position after the last character
        let adv = font.measure_str(&self.content, None).0;
        if (adv - x_in_inner).abs() < best_dist {
            best_pos = self.content.len();
        }
        best_pos
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Return `self.cursor` clamped to a valid char boundary within `content`.
    fn clamped_cursor(&self) -> usize {
        let mut pos = self.cursor.min(self.content.len());
        while pos > 0 && !self.content.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    /// Find the byte index of the char boundary immediately before `pos`.
    fn prev_char_boundary(&self, pos: usize) -> usize {
        if pos == 0 {
            return 0;
        }
        let mut p = pos - 1;
        while p > 0 && !self.content.is_char_boundary(p) {
            p -= 1;
        }
        p
    }
}
// }}}
