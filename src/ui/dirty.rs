//! Dirty-region tracking for the partial-redraw system.

/// Describes which part of the retained surface needs to be repainted.
///
/// Accumulated per event cycle via [`DirtyRegion::merge`].  At
/// `RedrawRequested` time, `Application` checks this value and only repaints
/// the required region, skipping unchanged parts of the frame.
#[derive(Clone, Copy, PartialEq)]
pub enum DirtyRegion {
    /// Nothing changed; skip the repaint entirely.
    None,
    /// The entire frame must be redrawn.
    All,
    /// Only the back-button area changed (e.g. hover state toggled).
    BackButtonOnly,
    /// The main page content changed (e.g. divider dragged).
    PageOnly,
}

impl DirtyRegion {
    /// Combines two dirty regions into the least-specific region that covers both.
    ///
    /// `None` is the identity element; two different non-`None` regions escalate to `All`.
    pub fn merge(self, other: DirtyRegion) -> DirtyRegion {
        match (self, other) {
            (DirtyRegion::None, x) | (x, DirtyRegion::None) => x,
            (DirtyRegion::All, _) | (_, DirtyRegion::All) => DirtyRegion::All,
            (a, b) if a == b => a,
            _ => DirtyRegion::All,
        }
    }
}
