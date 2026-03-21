//! Dirty-region tracking for the partial-redraw system.

/// Describes which part of the retained surface needs to be repainted.
///
/// Accumulated per event cycle via [`DirtyRegion::merge`].  At
/// `RedrawRequested` time, `Application` checks this value and only repaints
/// the required region, skipping unchanged parts of the frame.
#[derive(Clone, Copy, PartialEq, Debug)]
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

// ── Implementation ──────────────────────────────────────────────────────────── {{{
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
// }}}

#[cfg(test)]
// ── Tests ──────────────────────────────────────────────────────────────── {{{
mod tests {
    use super::*;

    #[test]
    fn none_is_identity() {
        for region in [
            DirtyRegion::None,
            DirtyRegion::All,
            DirtyRegion::BackButtonOnly,
            DirtyRegion::PageOnly,
        ] {
            assert_eq!(DirtyRegion::None.merge(region), region);
            assert_eq!(region.merge(DirtyRegion::None), region);
        }
    }

    #[test]
    fn same_region_is_idempotent() {
        assert_eq!(DirtyRegion::All.merge(DirtyRegion::All), DirtyRegion::All);
        assert_eq!(
            DirtyRegion::PageOnly.merge(DirtyRegion::PageOnly),
            DirtyRegion::PageOnly
        );
        assert_eq!(
            DirtyRegion::BackButtonOnly.merge(DirtyRegion::BackButtonOnly),
            DirtyRegion::BackButtonOnly
        );
    }

    #[test]
    fn different_regions_escalate_to_all() {
        assert_eq!(
            DirtyRegion::PageOnly.merge(DirtyRegion::BackButtonOnly),
            DirtyRegion::All
        );
        assert_eq!(
            DirtyRegion::BackButtonOnly.merge(DirtyRegion::PageOnly),
            DirtyRegion::All
        );
    }

    #[test]
    fn all_absorbs_everything() {
        for region in [
            DirtyRegion::None,
            DirtyRegion::PageOnly,
            DirtyRegion::BackButtonOnly,
        ] {
            assert_eq!(DirtyRegion::All.merge(region), DirtyRegion::All);
            assert_eq!(region.merge(DirtyRegion::All), DirtyRegion::All);
        }
    }
}
// }}}
