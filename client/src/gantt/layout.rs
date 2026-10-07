//! Where things are on the Gantt, as numbers: which day is at which x, which row at which y. The
//! drawing and the pointer handling both go through here, so what is drawn is what is hit.

use std::ops::Range;

use chrono::{Datelike, NaiveDate};

pub const HEADER_HEIGHT: f64 = 44.0;
pub const ROW_HEIGHT: f64 = 30.0;
pub const MIN_DAY_WIDTH: f64 = 3.0;
pub const MAX_DAY_WIDTH: f64 = 90.0;

/// A day as a number, so positions between days can be told apart
pub fn day_number(date: NaiveDate) -> f64 {
    f64::from(date.num_days_from_ce())
}

pub fn date_of(day: f64) -> Option<NaiveDate> {
    NaiveDate::from_num_days_from_ce_opt(day.floor() as i32)
}

/// What part of the timeline is on screen
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The day at the left edge of the timeline
    pub origin: f64,
    pub day_width: f64,
    /// How far the rows are scrolled up
    pub scroll_y: f64,
}

impl View {
    /// Shows `first` to `last` across `width` pixels, with a little room either side
    pub fn fit(first: NaiveDate, last: NaiveDate, width: f64) -> Self {
        let days = (day_number(last) - day_number(first)).max(7.0) + 4.0;
        Self {
            origin: day_number(first) - 2.0,
            day_width: (width / days).clamp(MIN_DAY_WIDTH, MAX_DAY_WIDTH),
            scroll_y: 0.0,
        }
    }

    pub fn x(&self, day: f64) -> f64 {
        (day - self.origin) * self.day_width
    }

    pub fn day_at(&self, x: f64) -> f64 {
        self.origin + x / self.day_width
    }

    /// Moves the view so what was under the pointer moves with it
    pub fn pan(&mut self, dx: f64, dy: f64, rows: usize, height: f64) {
        self.origin -= dx / self.day_width;
        self.scroll_y += dy;
        self.clamp(rows, height);
    }

    /// Zooms in by `factor` keeping the day at pixel `x` where it is
    pub fn zoom(&mut self, x: f64, factor: f64) {
        let day = self.day_at(x);
        self.day_width = (self.day_width * factor).clamp(MIN_DAY_WIDTH, MAX_DAY_WIDTH);
        self.origin = day - x / self.day_width;
    }

    /// Keeps the rows from scrolling past the first or the last
    pub fn clamp(&mut self, rows: usize, height: f64) {
        let content = rows as f64 * ROW_HEIGHT;
        let room = (height - HEADER_HEIGHT).max(0.0);
        self.scroll_y = self.scroll_y.clamp(-(content - room).max(0.0), 0.0);
    }

    /// The rows that show in a canvas `height` tall
    pub fn visible_rows(&self, height: f64, rows: usize) -> Range<usize> {
        let first = ((-self.scroll_y) / ROW_HEIGHT).floor().max(0.0) as usize;
        let count = ((height - HEADER_HEIGHT) / ROW_HEIGHT).ceil().max(0.0) as usize + 1;
        first.min(rows)..(first + count).min(rows)
    }

    pub fn row_top(&self, row: usize) -> f64 {
        HEADER_HEIGHT + row as f64 * ROW_HEIGHT + self.scroll_y
    }

    pub fn row_at(&self, y: f64, rows: usize) -> Option<usize> {
        if y < HEADER_HEIGHT {
            return None;
        }
        let row = ((y - HEADER_HEIGHT - self.scroll_y) / ROW_HEIGHT).floor();
        (row >= 0.0 && (row as usize) < rows).then_some(row as usize)
    }

    /// The first and last whole days on screen in a timeline `width` wide
    pub fn days(&self, width: f64) -> Range<i64> {
        self.origin.floor() as i64..self.day_at(width).ceil() as i64
    }
}

/// The bar for a span: its left edge and width in pixels, at least wide enough to see
pub fn bar(view: &View, span: (NaiveDate, NaiveDate)) -> (f64, f64) {
    let x = view.x(day_number(span.0));
    let width = (view.x(day_number(span.1)) - x).max(4.0);
    (x, width)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2024, 1, day).unwrap()
    }

    #[test]
    fn days_and_pixels_convert_both_ways() {
        let view = View::fit(d(1), d(21), 600.0);
        let x = view.x(day_number(d(10)));
        assert_eq!(date_of(view.day_at(x + 1.0)), Some(d(10)));
        assert!(view.x(day_number(d(1))) > 0.0, "room before the first day");
        assert!(view.x(day_number(d(21))) < 600.0, "and after the last");
    }

    #[test]
    fn zooming_keeps_the_day_under_the_pointer_still() {
        let mut view = View::fit(d(1), d(30), 900.0);
        let day = view.day_at(300.0);
        view.zoom(300.0, 2.0);
        assert!((view.day_at(300.0) - day).abs() < 1e-9);
        view.zoom(300.0, 1000.0);
        assert_eq!(view.day_width, MAX_DAY_WIDTH);
        view.zoom(300.0, 0.0001);
        assert_eq!(view.day_width, MIN_DAY_WIDTH);
    }

    #[test]
    fn rows_are_found_from_the_pointer_and_scrolling_is_kept_in_range() {
        let mut view = View::fit(d(1), d(30), 900.0);
        assert_eq!(view.row_at(HEADER_HEIGHT - 1.0, 10), None);
        assert_eq!(view.row_at(HEADER_HEIGHT + 1.0, 10), Some(0));
        assert_eq!(view.row_at(HEADER_HEIGHT + ROW_HEIGHT * 2.5, 10), Some(2));
        assert_eq!(view.row_at(HEADER_HEIGHT + ROW_HEIGHT * 11.0, 10), None);

        view.pan(0.0, -10_000.0, 10, 300.0);
        let last_room = 10.0 * ROW_HEIGHT - (300.0 - HEADER_HEIGHT);
        assert_eq!(view.scroll_y, -last_room);
        view.pan(0.0, 10_000.0, 10, 300.0);
        assert_eq!(view.scroll_y, 0.0);
        view.pan(0.0, -5.0, 3, 600.0);
        assert_eq!(view.scroll_y, 0.0, "everything fits, nothing scrolls");
    }

    #[test]
    fn only_rows_that_show_are_visible() {
        let mut view = View::fit(d(1), d(30), 900.0);
        assert_eq!(
            view.visible_rows(HEADER_HEIGHT + ROW_HEIGHT * 3.0, 100),
            0..4
        );
        view.pan(0.0, -ROW_HEIGHT * 10.0, 100, 400.0);
        assert!(view.visible_rows(400.0, 100).start >= 10);
        assert_eq!(view.visible_rows(400.0, 5), 5..5);
    }

    #[test]
    fn bars_are_never_too_thin_to_see() {
        let view = View::fit(d(1), d(30), 900.0);
        let (x, width) = bar(&view, (d(5), d(5)));
        assert!(x > 0.0 && width >= 4.0);
        let (_, week) = bar(&view, (d(1), d(8)));
        assert!((week - 7.0 * view.day_width).abs() < 1e-9);
    }
}
