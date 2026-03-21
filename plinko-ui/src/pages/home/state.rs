//! Mutable state for the home page.

/// Tracks which navigation card (0 = Daily, 1 = Overview, 2 = Settings) is
/// currently under the cursor, or `None` if none is hovered.
pub struct HomeState {
    pub hovered_card: Option<usize>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl HomeState {
    pub fn new() -> Self {
        Self { hovered_card: None }
    }
}
// }}}
