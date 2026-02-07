pub mod daily;
pub mod planning;
pub mod settings;

use skia_safe::Canvas;

use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

#[derive(Clone, Copy, PartialEq)]
pub enum PageId {
    Daily,
    Planning,
    Settings,
}

pub trait Page {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache);
    fn on_cursor_moved(&mut self, x: f32, y: f32, width: f32, height: f32) -> DirtyRegion;
    fn on_mouse_input(&mut self, x: f32, y: f32, pressed: bool, width: f32, height: f32) -> DirtyRegion;
}

pub struct PageManager {
    pub active: PageId,
    pub daily: daily::DailyPage,
    pub planning: planning::PlanningPage,
    pub settings: settings::SettingsPage,
}

impl PageManager {
    pub fn new() -> Self {
        Self {
            active: PageId::Daily,
            daily: daily::DailyPage::new(),
            planning: planning::PlanningPage::new(),
            settings: settings::SettingsPage::new(),
        }
    }

    pub fn active_page(&self) -> &dyn Page {
        match self.active {
            PageId::Daily => &self.daily,
            PageId::Planning => &self.planning,
            PageId::Settings => &self.settings,
        }
    }

    pub fn active_page_mut(&mut self) -> &mut dyn Page {
        match self.active {
            PageId::Daily => &mut self.daily,
            PageId::Planning => &mut self.planning,
            PageId::Settings => &mut self.settings,
        }
    }

    pub fn set_active(&mut self, page: PageId) {
        self.active = page;
    }
}
