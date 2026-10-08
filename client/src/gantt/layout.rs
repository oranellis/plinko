//! Where things are on the chart, as numbers, the way Plinko 1 laid it out: day 0 is the day the
//! plan starts, and everything is a day offset from it. Drawing and the pointer both go through
//! here, so what is drawn is what is hit.

pub const ROW_HEIGHT: f64 = 36.0;
pub const HEADER_HEIGHT: f64 = 56.0;
/// The month row of the header, the day row is the rest
pub const MONTH_HEIGHT: f64 = 22.0;
pub const DAY_WIDTH: f64 = 34.0;
pub const MIN_DAY_WIDTH: f64 = 8.0;
pub const MAX_DAY_WIDTH: f64 = 80.0;
/// Room above and below a bar in its row
pub const BAR_INSET_Y: f64 = 6.0;
/// The thinnest a bar is drawn
pub const MIN_BAR_WIDTH: f64 = 4.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// The size of the canvas and how much there is to scroll over
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub width: f64,
    pub height: f64,
    pub lanes: usize,
    /// The day offset after the last day anything is on
    pub last_day: f64,
}

/// Where the chart is scrolled to and how far it is zoomed
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Pixels the days are scrolled left of day 0
    pub scroll_x: f64,
    /// Pixels the lanes are scrolled up
    pub scroll_y: f64,
    pub day_width: f64,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            day_width: DAY_WIDTH,
        }
    }
}

impl Viewport {
    /// The left edge of a day offset
    pub fn x(&self, day: f64) -> f64 {
        day * self.day_width - self.scroll_x
    }

    /// The day offset at a pixel
    pub fn day_at(&self, x: f64) -> f64 {
        (self.scroll_x + x) / self.day_width
    }

    pub fn lane_top(&self, lane: usize) -> f64 {
        HEADER_HEIGHT + lane as f64 * ROW_HEIGHT - self.scroll_y
    }

    /// The days shown across `width` pixels, the first and the last, for drawing the header
    pub fn visible_days(&self, width: f64) -> (i64, i64) {
        let first = (self.scroll_x / self.day_width).floor() as i64;
        let last = ((self.scroll_x + width) / self.day_width).ceil() as i64;
        (first, last)
    }

    /// Keeps the chart in range: half a screen before the plan starts, half a screen after the last
    /// day, and no further than the last lane
    pub fn clamp(&mut self, b: Bounds) {
        let right = (b.last_day * self.day_width + b.width / 2.0).max(0.0);
        self.scroll_x = self
            .scroll_x
            .clamp(-b.width / 2.0, right.max(-b.width / 2.0));
        let down = (b.lanes as f64 * ROW_HEIGHT - (b.height - HEADER_HEIGHT)).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, down);
    }

    /// Drags the chart by `dx`, `dy` pixels of pointer movement
    pub fn drag(&mut self, dx: f64, dy: f64, b: Bounds) {
        self.scroll(-dx, -dy, b);
    }

    /// Scrolls it by `dx`, `dy` pixels, as the wheel does
    pub fn scroll(&mut self, dx: f64, dy: f64, b: Bounds) {
        self.scroll_x += dx;
        self.scroll_y += dy;
        self.clamp(b);
    }

    /// Zooms by `factor` keeping the day under the pixel `x` where it is
    pub fn zoom(&mut self, x: f64, factor: f64, b: Bounds) {
        let day = self.day_at(x);
        self.day_width = (self.day_width * factor).clamp(MIN_DAY_WIDTH, MAX_DAY_WIDTH);
        self.scroll_x = day * self.day_width - x;
        self.clamp(b);
    }

    /// Puts a day in the middle of the chart
    pub fn centre_on_day(&mut self, day: f64, b: Bounds) {
        self.scroll_x = day * self.day_width - b.width / 2.0;
        self.clamp(b);
    }

    /// Puts a lane in the middle of the rows
    pub fn centre_on_lane(&mut self, lane: usize, b: Bounds) {
        let middle = lane as f64 * ROW_HEIGHT + ROW_HEIGHT / 2.0;
        self.scroll_y = middle - (b.height - HEADER_HEIGHT) / 2.0;
        self.clamp(b);
    }

    fn inset_x(&self) -> f64 {
        self.day_width * 0.12
    }

    /// The rounded bar of something from day offset `start` to the day offset after its last
    pub fn bar(&self, start: f64, end: f64, lane: usize) -> Rect {
        let inset = self.inset_x();
        Rect {
            x: self.x(start) + inset,
            y: self.lane_top(lane) + BAR_INSET_Y,
            w: ((end - start) * self.day_width - 2.0 * inset).max(MIN_BAR_WIDTH),
            h: ROW_HEIGHT - 2.0 * BAR_INSET_Y,
        }
    }

    /// The middle of the first day, for a diamond or where a line starts
    pub fn start_centre(&self, start: f64, lane: usize) -> (f64, f64) {
        (
            self.x(start) + self.day_width / 2.0,
            self.lane_top(lane) + ROW_HEIGHT / 2.0,
        )
    }

    /// The middle of the last day of something that ends on the day offset `end`
    pub fn end_centre(&self, end: f64, lane: usize) -> (f64, f64) {
        self.start_centre(end - 1.0, lane)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            width: 800.0,
            height: 400.0,
            lanes: 20,
            last_day: 60.0,
        }
    }

    #[test]
    fn days_are_offsets_from_the_start_of_the_plan() {
        let view = Viewport {
            scroll_x: 100.0,
            ..Viewport::default()
        };
        assert_eq!(view.x(10.0), 240.0);
        assert_eq!(view.day_at(240.0), 10.0);
        assert_eq!(view.lane_top(0), HEADER_HEIGHT);
        assert_eq!(view.lane_top(2), HEADER_HEIGHT + 2.0 * ROW_HEIGHT);
    }

    #[test]
    fn the_chart_scrolls_half_a_screen_past_either_end_and_no_further() {
        let b = bounds();
        let mut view = Viewport::default();
        view.drag(10_000.0, 10_000.0, b);
        assert_eq!(view.scroll_x, -400.0);
        assert_eq!(view.scroll_y, 0.0);
        view.drag(-100_000.0, -100_000.0, b);
        assert_eq!(view.scroll_x, 60.0 * DAY_WIDTH + 400.0);
        assert_eq!(view.scroll_y, 20.0 * ROW_HEIGHT - (400.0 - HEADER_HEIGHT));
        // few lanes means no vertical scrolling at all
        let mut short = Viewport::default();
        short.scroll(0.0, 500.0, Bounds { lanes: 2, ..b });
        assert_eq!(short.scroll_y, 0.0);
    }

    #[test]
    fn zooming_keeps_the_day_under_the_pointer_and_stays_in_range() {
        let b = bounds();
        let mut view = Viewport {
            scroll_x: 500.0,
            ..Viewport::default()
        };
        let day = view.day_at(300.0);
        view.zoom(300.0, 1.5, b);
        assert!((view.day_at(300.0) - day).abs() < 1e-9);
        view.zoom(300.0, 1000.0, b);
        assert_eq!(view.day_width, MAX_DAY_WIDTH);
        view.zoom(300.0, 0.0001, b);
        assert_eq!(view.day_width, MIN_DAY_WIDTH);
    }

    #[test]
    fn centring_puts_the_day_and_the_lane_in_the_middle() {
        let b = bounds();
        let mut view = Viewport::default();
        view.centre_on_day(30.0, b);
        assert!((view.x(30.0) - 400.0).abs() < 1e-9);
        view.centre_on_lane(10, b);
        let middle = view.lane_top(10) + ROW_HEIGHT / 2.0;
        assert!((middle - (HEADER_HEIGHT + (400.0 - HEADER_HEIGHT) / 2.0)).abs() < 1e-9);
    }

    #[test]
    fn bars_are_inset_rounded_boxes_and_never_too_thin() {
        let view = Viewport::default();
        let bar = view.bar(2.0, 5.0, 1);
        // three days wide less the inset either side, 24 high in a 36 row
        assert!((bar.w - (3.0 * DAY_WIDTH - 2.0 * 0.12 * DAY_WIDTH)).abs() < 1e-9);
        assert_eq!(bar.h, 24.0);
        assert_eq!(bar.y, HEADER_HEIGHT + ROW_HEIGHT + BAR_INSET_Y);
        assert!(view.bar(2.0, 2.0, 0).w >= MIN_BAR_WIDTH);
        let (x, y) = view.start_centre(2.0, 1);
        assert_eq!(
            (x, y),
            (
                2.0 * DAY_WIDTH + DAY_WIDTH / 2.0,
                HEADER_HEIGHT + ROW_HEIGHT * 1.5
            )
        );
        assert_eq!(view.end_centre(5.0, 1).0, 4.0 * DAY_WIDTH + DAY_WIDTH / 2.0);
    }
}
