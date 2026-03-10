//! Mutable state for the overview page.

/// Full interactive state for the overview page.
pub struct OverviewState {
    /// Hovered page-specific toolbar button index, if any.
    pub toolbar_btn_hovered: Option<usize>,
}

impl OverviewState {
    pub fn new() -> Self {
        Self {
            toolbar_btn_hovered: None,
        }
    }
}
