//! Multi-line text input component with vertical scroll and basic editing.

use std::cell::Cell;

pub struct MultiLineInput {
    pub content: String,
    /// Byte-index cursor, always on a char boundary.
    pub cursor: usize,
    pub focused: bool,
    /// Vertical scroll offset in pixels (interior-mutable for render access).
    pub scroll_y: Cell<f32>,
}

impl MultiLineInput {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let cursor = content.len();
        Self {
            content,
            cursor,
            focused: false,
            scroll_y: Cell::new(0.0),
        }
    }

    pub fn set_content(&mut self, s: impl Into<String>) {
        self.content = s.into();
        self.cursor = self.content.len();
        self.scroll_y.set(0.0);
    }

    fn clamped_cursor(&self) -> usize {
        let mut pos = self.cursor.min(self.content.len());
        while pos > 0 && !self.content.is_char_boundary(pos) {
            pos -= 1;
        }
        pos
    }

    fn prev_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos.saturating_sub(1);
        while p > 0 && !self.content.is_char_boundary(p) {
            p -= 1;
        }
        p
    }

    fn next_char_boundary(&self, pos: usize) -> usize {
        let mut p = pos + 1;
        while p < self.content.len() && !self.content.is_char_boundary(p) {
            p += 1;
        }
        p.min(self.content.len())
    }

    pub fn insert_char(&mut self, ch: char) {
        let cursor = self.clamped_cursor();
        self.content.insert(cursor, ch);
        self.cursor = cursor + ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    pub fn backspace(&mut self) {
        let cursor = self.clamped_cursor();
        if cursor == 0 {
            return;
        }
        let prev = self.prev_char_boundary(cursor);
        self.content.drain(prev..cursor);
        self.cursor = prev;
    }

    pub fn move_left(&mut self) {
        let cursor = self.clamped_cursor();
        self.cursor = self.prev_char_boundary(cursor);
    }

    pub fn move_right(&mut self) {
        let cursor = self.clamped_cursor();
        self.cursor = self.next_char_boundary(cursor);
    }

    pub fn move_to_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_to_end(&mut self) {
        self.cursor = self.content.len();
    }

    /// Given a click position relative to the box top-left (not the rect,
    /// but the inner area — i.e. x=0 is at rect.left+8, y=0 is at rect.top+4),
    /// return the closest byte cursor position.
    pub fn cursor_for_click(&self, x_in_box: f32, y_in_box: f32, font: &skia_safe::Font) -> usize {
        let (_, metrics) = font.metrics();
        let line_h = metrics.descent - metrics.ascent + 2.0;
        let scroll_y = self.scroll_y.get();

        let y_in_text = (y_in_box - 4.0 + scroll_y).max(0.0);
        let clicked_line = (y_in_text / line_h) as usize;

        let lines: Vec<&str> = if self.content.is_empty() {
            vec![""]
        } else {
            self.content.split('\n').collect()
        };

        let line_idx = clicked_line.min(lines.len().saturating_sub(1));
        let line = lines[line_idx];

        // Byte offset of start of this line
        let mut byte_offset = 0usize;
        for line_str in lines.iter().take(line_idx) {
            byte_offset += line_str.len() + 1; // +1 for '\n'
        }

        // Find best x position within line
        let x_in_line = (x_in_box - 8.0).max(0.0);
        let mut best_pos = 0usize;
        let mut best_dist = x_in_line.abs();

        for (i, _) in line.char_indices() {
            let adv = font.measure_str(&line[..i], None).0;
            let dist = (adv - x_in_line).abs();
            if dist < best_dist {
                best_dist = dist;
                best_pos = i;
            }
        }
        let adv = font.measure_str(line, None).0;
        if (adv - x_in_line).abs() < best_dist {
            best_pos = line.len();
        }

        (byte_offset + best_pos).min(self.content.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(s: &str) -> MultiLineInput {
        MultiLineInput::new(s)
    }

    // ── Basic editing ─────────────────────────────────────────────────────────

    #[test]
    fn new_cursor_at_end() {
        let m = input("hello");
        assert_eq!(m.cursor, 5);
    }

    #[test]
    fn insert_char_advances_cursor() {
        let mut m = input("");
        m.insert_char('a');
        assert_eq!(m.content, "a");
        assert_eq!(m.cursor, 1);
        m.insert_char('b');
        assert_eq!(m.content, "ab");
        assert_eq!(m.cursor, 2);
    }

    #[test]
    fn insert_char_in_middle() {
        let mut m = input("ac");
        m.cursor = 1; // between 'a' and 'c'
        m.insert_char('b');
        assert_eq!(m.content, "abc");
        assert_eq!(m.cursor, 2);
    }

    #[test]
    fn insert_newline() {
        let mut m = input("ab");
        m.cursor = 1;
        m.insert_newline();
        assert_eq!(m.content, "a\nb");
        assert_eq!(m.cursor, 2);
    }

    #[test]
    fn backspace_removes_previous_char() {
        let mut m = input("abc");
        m.backspace();
        assert_eq!(m.content, "ab");
        assert_eq!(m.cursor, 2);
    }

    #[test]
    fn backspace_at_start_is_noop() {
        let mut m = input("abc");
        m.cursor = 0;
        m.backspace();
        assert_eq!(m.content, "abc");
        assert_eq!(m.cursor, 0);
    }

    #[test]
    fn backspace_removes_newline() {
        let mut m = input("a\nb");
        m.cursor = 2; // after '\n'
        m.backspace();
        assert_eq!(m.content, "ab");
        assert_eq!(m.cursor, 1);
    }

    // ── Cursor movement ───────────────────────────────────────────────────────

    #[test]
    fn move_left_and_right() {
        let mut m = input("abc");
        m.move_left();
        assert_eq!(m.cursor, 2);
        m.move_left();
        assert_eq!(m.cursor, 1);
        m.move_right();
        assert_eq!(m.cursor, 2);
        m.move_right();
        assert_eq!(m.cursor, 3);
    }

    #[test]
    fn move_left_clamps_at_zero() {
        let mut m = input("x");
        m.cursor = 0;
        m.move_left();
        assert_eq!(m.cursor, 0);
    }

    #[test]
    fn move_right_clamps_at_end() {
        let mut m = input("x");
        m.move_right();
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn move_to_start_and_end() {
        let mut m = input("hello world");
        m.cursor = 5;
        m.move_to_start();
        assert_eq!(m.cursor, 0);
        m.move_to_end();
        assert_eq!(m.cursor, 11);
    }

    // ── Unicode safety ────────────────────────────────────────────────────────

    #[test]
    fn insert_multibyte_char() {
        let mut m = input("");
        m.insert_char('é'); // 2 bytes in UTF-8
        assert_eq!(m.content, "é");
        assert_eq!(m.cursor, 'é'.len_utf8());
    }

    #[test]
    fn backspace_multibyte() {
        let mut m = input("aé");
        m.backspace(); // removes 'é' (2 bytes)
        assert_eq!(m.content, "a");
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn cursor_always_on_char_boundary() {
        let mut m = MultiLineInput::new("héllo");
        // Force cursor to the middle of 'é' (byte index 2, which is not a boundary)
        m.cursor = 2;
        m.move_left(); // clamped_cursor should step back to a boundary
        assert!(m.content.is_char_boundary(m.cursor));
    }

    // ── set_content ───────────────────────────────────────────────────────────

    #[test]
    fn set_content_resets_cursor_and_scroll() {
        let mut m = input("old content");
        m.cursor = 3;
        m.scroll_y.set(50.0);
        m.set_content("new");
        assert_eq!(m.content, "new");
        assert_eq!(m.cursor, 3);
        assert_eq!(m.scroll_y.get(), 0.0);
    }

    #[test]
    fn set_content_empty_string() {
        let mut m = input("something");
        m.set_content("");
        assert_eq!(m.content, "");
        assert_eq!(m.cursor, 0);
    }
}
