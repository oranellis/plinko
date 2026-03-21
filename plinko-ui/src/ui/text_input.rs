//! A simple single-line text input component with cursor navigation.

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
