//! A simple single-line text input component with cursor navigation.

/// Single-line text input with a byte-index cursor.
pub struct TextInput {
    pub content: String,
    /// Byte index into `content`. Always on a valid UTF-8 char boundary.
    pub cursor: usize,
    pub focused: bool,
}

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
        }
    }

    /// Replace the content and reset the cursor to the end.
    pub fn set_content(&mut self, s: impl Into<String>) {
        self.content = s.into();
        self.cursor = self.content.len();
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
