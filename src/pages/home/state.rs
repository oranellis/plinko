pub struct HomeState {
    pub hovered_card: Option<usize>,
}

impl HomeState {
    pub fn new() -> Self {
        Self { hovered_card: None }
    }
}
