#[derive(Clone, Copy, PartialEq)]
pub enum DirtyRegion {
    None,
    All,
    BackButtonOnly,
    PageOnly,
}

impl DirtyRegion {
    pub fn merge(self, other: DirtyRegion) -> DirtyRegion {
        match (self, other) {
            (DirtyRegion::None, x) | (x, DirtyRegion::None) => x,
            (DirtyRegion::All, _) | (_, DirtyRegion::All) => DirtyRegion::All,
            (a, b) if a == b => a,
            _ => DirtyRegion::All,
        }
    }
}
