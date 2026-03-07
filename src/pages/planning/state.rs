//! Mutable state for the planning page.

/// Interactive state for the planning page's split-panel divider.
pub struct PlanningState {
    /// Fraction of the page width at which the divider sits (0.0–1.0, clamped 0.1–0.9).
    pub divider_ratio: f32,
    /// `true` while the user holds the mouse button down on the divider.
    pub dragging_divider: bool,
    /// `true` when the cursor is within the divider's hit area.
    pub hovering_divider: bool,
}

impl PlanningState {
    pub fn new() -> Self {
        Self {
            divider_ratio: 0.5,
            dragging_divider: false,
            hovering_divider: false,
        }
    }
}
