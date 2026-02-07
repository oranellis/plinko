pub struct PlanningState {
    pub divider_ratio: f32,
    pub dragging_divider: bool,
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
