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
