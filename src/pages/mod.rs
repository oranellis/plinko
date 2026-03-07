//! Page system: trait definition, page IDs, and the [`PageManager`] dispatcher.
//!
//! Each page lives in its own sub-module with three files:
//! - `mod.rs`    — the page struct implementing [`Page`].
//! - `state.rs`  — mutable page-specific state.
//! - `render.rs` — stateless Skia drawing functions.
//!
//! Navigation between pages is handled by [`crate::app::Application`]; the
//! pages themselves only report [`DirtyRegion`] changes in response to input.

pub mod daily;
pub mod home;
pub mod planning;
pub mod settings;

use skia_safe::Canvas;

use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;

/// Identifies one of the application's top-level pages.
#[derive(Clone, Copy, PartialEq)]
pub enum PageId {
    Home,
    Daily,
    Planning,
    Settings,
}

/// Common interface that every full-screen page must implement.
pub trait Page {
    /// Draw the page content onto `canvas`.  `width`/`height` are logical pixels.
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache);
    /// Called on every [`WindowEvent::CursorMoved`] while this page is active.
    /// Returns which region (if any) needs repainting.
    fn on_cursor_moved(&mut self, x: f32, y: f32, width: f32, height: f32) -> DirtyRegion;
    /// Called on left mouse button press (`pressed = true`) and release (`pressed = false`).
    /// Returns which region (if any) needs repainting.
    fn on_mouse_input(&mut self, x: f32, y: f32, pressed: bool, width: f32, height: f32) -> DirtyRegion;
}

/// Owns all page instances and tracks the currently active one.
///
/// The active page receives input events and is rendered each frame.
pub struct PageManager {
    pub active: PageId,
    pub home: home::HomePage,
    pub daily: daily::DailyPage,
    pub planning: planning::PlanningPage,
    pub settings: settings::SettingsPage,
}

impl PageManager {
    pub fn new() -> Self {
        Self {
            active: PageId::Home,
            home: home::HomePage::new(),
            daily: daily::DailyPage::new(),
            planning: planning::PlanningPage::new(),
            settings: settings::SettingsPage::new(),
        }
    }

    /// Returns a shared reference to the active page as a [`Page`] trait object.
    pub fn active_page(&self) -> &dyn Page {
        match self.active {
            PageId::Home => &self.home,
            PageId::Daily => &self.daily,
            PageId::Planning => &self.planning,
            PageId::Settings => &self.settings,
        }
    }

    /// Returns a mutable reference to the active page as a [`Page`] trait object.
    pub fn active_page_mut(&mut self) -> &mut dyn Page {
        match self.active {
            PageId::Home => &mut self.home,
            PageId::Daily => &mut self.daily,
            PageId::Planning => &mut self.planning,
            PageId::Settings => &mut self.settings,
        }
    }

    /// Changes the active page.  Does not reset page state.
    pub fn set_active(&mut self, page: PageId) {
        self.active = page;
    }
}
