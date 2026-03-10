//! Mutable state for the overview page.

/// Full interactive state for the overview page.
pub struct OverviewState {
    /// Hovered page-specific toolbar button index, if any.
    pub toolbar_btn_hovered: Option<usize>,
    /// Set when the users toolbar button is clicked; consumed by `take_open_request`.
    pub open_users_window: bool,
}

impl OverviewState {
    pub fn new() -> Self {
        Self {
            toolbar_btn_hovered: None,
            open_users_window: false,
        }
    }
}
