//! Multi-line text input component with vertical scroll, word-wrap, and editing.

use std::cell::Cell;
use std::ops::Range;

use skia_safe::Font;

// ── Data types ────────────────────────────────────────────────────────────────

/// A single rendered line produced by word-wrapping.
pub struct VisualLine<'a> {
    /// Slice of the original content string for this line (no trailing newline).
    pub text: &'a str,
    /// Byte offset of `text` within the full content string.
    pub byte_start: usize,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl VisualLine<'_> {
    pub fn byte_end(&self) -> usize {
        self.byte_start + self.text.len()
    }
}
// }}}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct MultiLineInput {
    pub content: String,
    /// Byte-index cursor position, always on a char boundary.
    pub cursor: usize,
    pub focused: bool,
    /// Vertical scroll offset in pixels (interior-mutable for render).
    pub scroll_y: Cell<f32>,
    /// Preserved horizontal pixel offset for up/down navigation.  `None` means
    /// use the cursor's current X when the first vertical move fires.
    pub x_hint: Option<f32>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl MultiLineInput {
    pub fn new(content: impl Into<String>) -> Self {
        let content = content.into();
        let cursor = content.len();
        Self {
            content,
            cursor,
            focused: false,
            scroll_y: Cell::new(0.0),
            x_hint: None,
        }
    }

    pub fn set_content(&mut self, s: impl Into<String>) {
        self.content = s.into();
        self.cursor = self.content.len();
        self.scroll_y.set(0.0);
        self.x_hint = None;
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    pub fn clamped_cursor(&self) -> usize {
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

    // ── Editing operations ────────────────────────────────────────────────────

    pub fn insert_char(&mut self, ch: char) {
        let cursor = self.clamped_cursor();
        self.content.insert(cursor, ch);
        self.cursor = cursor + ch.len_utf8();
        self.x_hint = None;
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
        self.x_hint = None;
    }

    pub fn move_left(&mut self) {
        let cursor = self.clamped_cursor();
        self.cursor = self.prev_char_boundary(cursor);
        self.x_hint = None;
    }

    pub fn move_right(&mut self) {
        let cursor = self.clamped_cursor();
        self.cursor = self.next_char_boundary(cursor);
        self.x_hint = None;
    }

    pub fn move_to_start(&mut self) {
        self.cursor = 0;
        self.x_hint = None;
    }

    pub fn move_to_end(&mut self) {
        self.cursor = self.content.len();
        self.x_hint = None;
    }

    // ── Word-wrapped visual line layout ───────────────────────────────────────

    /// Return the visual lines produced by wrapping `content` to `inner_width`
    /// pixels.  Hard newlines in the content always start a new visual line.
    pub fn visual_lines<'a>(&'a self, inner_width: f32, font: &Font) -> Vec<VisualLine<'a>> {
        let mut result = Vec::new();
        if self.content.is_empty() {
            result.push(VisualLine {
                text: "",
                byte_start: 0,
            });
            return result;
        }
        let mut byte_offset = 0usize;
        for (para_idx, para) in self.content.split('\n').enumerate() {
            if para_idx > 0 {
                byte_offset += 1; // the '\n' separator
            }
            wrap_paragraph(para, byte_offset, inner_width, font, &mut result);
            byte_offset += para.len();
        }
        result
    }

    /// Clamp `scroll_y` so it cannot scroll past the end of the content.
    /// Called during render (uses interior mutability).
    pub fn clamp_scroll(&self, inner_width: f32, font: &Font, line_h: f32, visible_h: f32) {
        let lines = self.visual_lines(inner_width, font);
        let total_h = lines.len() as f32 * line_h + 8.0;
        let max_scroll = (total_h - visible_h).max(0.0);
        let clamped = self.scroll_y.get().clamp(0.0, max_scroll);
        self.scroll_y.set(clamped);
    }

    /// Maximum scroll given the current content and box dimensions.
    pub fn max_scroll(&self, inner_width: f32, font: &Font, line_h: f32, visible_h: f32) -> f32 {
        let lines = self.visual_lines(inner_width, font);
        let total_h = lines.len() as f32 * line_h + 8.0;
        (total_h - visible_h).max(0.0)
    }

    /// Scroll just enough to keep the cursor inside the visible area.
    /// Only modifies `scroll_y` if the cursor would otherwise be off-screen.
    pub fn scroll_to_cursor(&self, inner_width: f32, font: &Font, line_h: f32, visible_h: f32) {
        let lines = self.visual_lines(inner_width, font);
        let cursor = self.clamped_cursor();
        let line_idx = lines
            .iter()
            .rposition(|l| l.byte_start <= cursor)
            .unwrap_or(0);
        let cursor_rel_y = line_idx as f32 * line_h;
        let scroll = self.scroll_y.get();
        let new_scroll = if cursor_rel_y < scroll {
            cursor_rel_y
        } else if cursor_rel_y + line_h > scroll + visible_h {
            cursor_rel_y + line_h - visible_h
        } else {
            scroll
        };
        self.scroll_y.set(new_scroll.max(0.0));
    }

    // ── Vertical cursor movement ──────────────────────────────────────────────

    pub fn move_up(&mut self, inner_width: f32, font: &Font) {
        // Collect owned data so we can mutate self afterwards.
        let cursor = self.clamped_cursor();
        let (line_idx, current_byte_start, current_text_len, prev_text_owned, prev_byte_start) = {
            let lines = self.visual_lines(inner_width, font);
            let idx = lines
                .iter()
                .rposition(|l| l.byte_start <= cursor)
                .unwrap_or(0);
            if idx == 0 {
                (0_usize, 0_usize, 0_usize, String::new(), 0_usize)
            } else {
                (
                    idx,
                    lines[idx].byte_start,
                    lines[idx].text.len(),
                    lines[idx - 1].text.to_owned(),
                    lines[idx - 1].byte_start,
                )
            }
        };

        if line_idx == 0 {
            self.cursor = 0;
            self.x_hint = None;
            return;
        }

        if self.x_hint.is_none() {
            let col_len = cursor
                .saturating_sub(current_byte_start)
                .min(current_text_len);
            let col_str = &self.content[current_byte_start..current_byte_start + col_len];
            self.x_hint = Some(font.measure_str(col_str, None).0);
        }
        let x = self.x_hint.unwrap();
        self.cursor = byte_at_x(&prev_text_owned, prev_byte_start, x, font);
    }

    pub fn move_down(&mut self, inner_width: f32, font: &Font) {
        let cursor = self.clamped_cursor();
        let content_len = self.content.len();
        let (
            line_idx,
            total_lines,
            current_byte_start,
            current_text_len,
            next_text_owned,
            next_byte_start,
        ) = {
            let lines = self.visual_lines(inner_width, font);
            let idx = lines
                .iter()
                .rposition(|l| l.byte_start <= cursor)
                .unwrap_or(0);
            let total = lines.len();
            if idx + 1 >= total {
                (idx, total, 0_usize, 0_usize, String::new(), 0_usize)
            } else {
                (
                    idx,
                    total,
                    lines[idx].byte_start,
                    lines[idx].text.len(),
                    lines[idx + 1].text.to_owned(),
                    lines[idx + 1].byte_start,
                )
            }
        };

        if line_idx + 1 >= total_lines {
            self.cursor = content_len;
            self.x_hint = None;
            return;
        }

        if self.x_hint.is_none() {
            let col_len = cursor
                .saturating_sub(current_byte_start)
                .min(current_text_len);
            let col_str = &self.content[current_byte_start..current_byte_start + col_len];
            self.x_hint = Some(font.measure_str(col_str, None).0);
        }
        let x = self.x_hint.unwrap();
        self.cursor = byte_at_x(&next_text_owned, next_byte_start, x, font);
    }

    // ── Click-to-cursor ───────────────────────────────────────────────────────

    /// Given a click position relative to the inner text area (x=0 at the
    /// 8px left padding, y=0 at the 4px top padding), return the closest byte
    /// cursor position, accounting for word-wrap visual lines.
    pub fn cursor_for_click(
        &self,
        x_in_box: f32,
        y_in_box: f32,
        inner_width: f32,
        font: &Font,
        line_h: f32,
    ) -> usize {
        let scroll_y = self.scroll_y.get();
        let y_in_text = (y_in_box - 4.0 + scroll_y).max(0.0);
        let clicked_line = (y_in_text / line_h) as usize;

        let lines = self.visual_lines(inner_width, font);
        let line_idx = clicked_line.min(lines.len().saturating_sub(1));
        let line = &lines[line_idx];

        let x_in_line = (x_in_box - 8.0).max(0.0);
        byte_at_x(line.text, line.byte_start, x_in_line, font)
    }

    // ── Link detection ────────────────────────────────────────────────────────

    /// Find all URL spans in `s`, returning byte ranges.
    pub fn find_links(s: &str) -> Vec<Range<usize>> {
        let prefixes = ["https://", "http://", "www."];
        let mut links = Vec::new();
        let mut i = 0usize;
        while i < s.len() {
            let rest = &s[i..];
            if let Some(prefix) = prefixes.iter().find(|&&p| rest.starts_with(p)) {
                let end_offset = rest
                    .char_indices()
                    .find(|(_, ch)| {
                        ch.is_whitespace() || matches!(*ch, '"' | '<' | '>' | ')' | ']')
                    })
                    .map(|(idx, _)| idx)
                    .unwrap_or(rest.len());
                if end_offset > prefix.len() {
                    links.push(i..i + end_offset);
                    i += end_offset;
                    continue;
                }
            }
            i += s[i..].chars().next().map_or(1, |c| c.len_utf8());
        }
        links
    }

    /// Open a URL in the system's default web browser.
    pub fn open_url(url: &str) {
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(url).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
}
// }}}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Find the byte offset within `line` (relative to `content` via `byte_start`)
/// whose rendered X position is closest to `target_x`.
fn byte_at_x(line: &str, byte_start: usize, target_x: f32, font: &Font) -> usize {
    let mut best_pos = 0usize;
    let mut best_dist = (font.measure_str("", None).0 - target_x).abs();

    for (i, _) in line.char_indices() {
        let adv = font.measure_str(&line[..i], None).0;
        let dist = (adv - target_x).abs();
        if dist < best_dist {
            best_dist = dist;
            best_pos = i;
        }
    }
    let adv = font.measure_str(line, None).0;
    if (adv - target_x).abs() < best_dist {
        best_pos = line.len();
    }

    (byte_start + best_pos).min(byte_start + line.len())
}

/// Break `para` (a single paragraph with no `\n`) into visual lines that fit
/// within `inner_width` pixels, appending to `result`.
fn wrap_paragraph<'a>(
    para: &'a str,
    para_start: usize,
    inner_width: f32,
    font: &Font,
    result: &mut Vec<VisualLine<'a>>,
) {
    if para.is_empty() {
        result.push(VisualLine {
            text: para,
            byte_start: para_start,
        });
        return;
    }

    let mut line_byte = 0usize;

    while line_byte < para.len() {
        let rest = &para[line_byte..];

        // Check if the whole remaining paragraph fits
        if font.measure_str(rest, None).0 <= inner_width {
            result.push(VisualLine {
                text: rest,
                byte_start: para_start + line_byte,
            });
            return;
        }

        // Find the last character that still fits on this line
        let mut char_end = 0usize; // byte end within `rest` of last fitting char
        let mut last_break: Option<usize> = None; // byte pos (in `rest`) after good break point

        for (byte_i, ch) in rest.char_indices() {
            let candidate_end = byte_i + ch.len_utf8();
            let w = font.measure_str(&rest[..candidate_end], None).0;
            if w > inner_width {
                break;
            }
            char_end = candidate_end;
            // Spaces and hyphens are good wrap points
            if ch == ' ' || ch == '-' {
                last_break = Some(candidate_end);
            }
        }

        // Decide where to break
        let break_at = if char_end == 0 {
            // Even the first character is too wide — include it anyway
            rest.chars().next().map_or(1, |c| c.len_utf8())
        } else if let Some(bp) = last_break {
            bp
        } else {
            char_end
        };

        result.push(VisualLine {
            text: &para[line_byte..line_byte + break_at],
            byte_start: para_start + line_byte,
        });

        // Skip a trailing space if we broke at one to avoid leading spaces
        let after = line_byte + break_at;
        let next = if after < para.len()
            && para.as_bytes().get(after) == Some(&b' ')
            && (last_break == Some(break_at))
        {
            after + 1
        } else {
            after
        };
        line_byte = next;
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
// ── Tests ──────────────────────────────────────────────────────────────── {{{
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
        m.cursor = 1;
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
        m.cursor = 2;
        m.backspace();
        assert_eq!(m.content, "ab");
        assert_eq!(m.cursor, 1);
    }

    // ── x_hint cleared by non-vertical movement ───────────────────────────────

    #[test]
    fn x_hint_cleared_on_horizontal_move() {
        let mut m = input("hello");
        m.x_hint = Some(42.0);
        m.move_left();
        assert!(m.x_hint.is_none());
    }

    #[test]
    fn x_hint_cleared_on_insert() {
        let mut m = input("hi");
        m.x_hint = Some(10.0);
        m.insert_char('!');
        assert!(m.x_hint.is_none());
    }

    #[test]
    fn x_hint_cleared_on_backspace() {
        let mut m = input("hi");
        m.x_hint = Some(10.0);
        m.backspace();
        assert!(m.x_hint.is_none());
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
        m.insert_char('é');
        assert_eq!(m.content, "é");
        assert_eq!(m.cursor, 'é'.len_utf8());
    }

    #[test]
    fn backspace_multibyte() {
        let mut m = input("aé");
        m.backspace();
        assert_eq!(m.content, "a");
        assert_eq!(m.cursor, 1);
    }

    #[test]
    fn cursor_always_on_char_boundary() {
        let mut m = MultiLineInput::new("héllo");
        m.cursor = 2; // inside 'é' (2 bytes)
        m.move_left();
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

    // ── find_links ────────────────────────────────────────────────────────────

    #[test]
    fn find_links_http() {
        let links = MultiLineInput::find_links("see http://example.com for info");
        assert_eq!(links.len(), 1);
        assert_eq!(
            &"see http://example.com for info"[links[0].clone()],
            "http://example.com"
        );
    }

    #[test]
    fn find_links_https() {
        let s = "visit https://github.com/org/repo";
        let links = MultiLineInput::find_links(s);
        assert_eq!(links.len(), 1);
        assert_eq!(&s[links[0].clone()], "https://github.com/org/repo");
    }

    #[test]
    fn find_links_www() {
        let s = "go to www.example.com today";
        let links = MultiLineInput::find_links(s);
        assert_eq!(links.len(), 1);
        assert_eq!(&s[links[0].clone()], "www.example.com");
    }

    #[test]
    fn find_links_multiple() {
        let s = "a https://one.com b http://two.com c";
        let links = MultiLineInput::find_links(s);
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn find_links_no_links() {
        assert!(MultiLineInput::find_links("just plain text").is_empty());
    }
}
// }}}
