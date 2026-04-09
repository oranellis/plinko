//! Page system: trait definition, page IDs, and the [`PageManager`] dispatcher.
//!
//! Each page lives in its own sub-module with three files:
//! - `mod.rs`    — the page struct implementing [`Page`].
//! - `state.rs`  — mutable page-specific state.
//! - `render.rs` — stateless Skia drawing functions.
//!
//! Navigation between pages is handled by [`crate::app::Application`]; the
//! pages themselves only report [`DirtyRegion`] changes in response to input.

pub mod allocation;
pub mod calendar_overrides;
pub mod daily;
pub mod home;
pub mod overview;
pub mod settings;

use skia_safe::Canvas;

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use plinko_shared::data::Plan;

/// Identifies one of the application's top-level pages.
#[derive(Clone, Copy, PartialEq)]
pub enum PageId {
    Home,
    Daily,
    Overview,
    Settings,
    Allocation,
    CalendarOverrides,
}

/// Common interface that every full-screen page must implement.
pub trait Page {
    /// Draw the page content onto `canvas`.  `width`/`height` are logical pixels.
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan);
    /// Called on every [`WindowEvent::CursorMoved`] while this page is active.
    /// Returns which region (if any) needs repainting.
    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> DirtyRegion;
    /// Called on left mouse button press (`pressed = true`) and release (`pressed = false`).
    /// Returns which region (if any) needs repainting.
    #[allow(clippy::too_many_arguments)]
    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        plan: &Plan,
        sender: &PlanRequestSender,
    ) -> DirtyRegion;
    /// Called on key-down events while this page is active.
    fn on_key_input(
        &mut self,
        _key: &winit::keyboard::Key,
        _sender: &PlanRequestSender,
    ) -> DirtyRegion {
        DirtyRegion::None
    }

    /// Called when the user pastes from the clipboard (Ctrl+V / Cmd+V).
    fn on_paste(&mut self, _text: &str, _sender: &PlanRequestSender) -> DirtyRegion {
        DirtyRegion::None
    }

    /// Called on mouse-wheel scroll events while this page is active.
    ///
    /// `delta_y` is positive = scroll up / zoom in, negative = scroll down / zoom out.
    /// `delta_x` is positive = scroll right, negative = scroll left (trackpad horizontal).
    /// `shift` is true when the Shift modifier is held.
    fn on_scroll(
        &mut self,
        _delta_y: f32,
        _delta_x: f32,
        _shift: bool,
        _width: f32,
        _height: f32,
        _plan: &Plan,
    ) -> DirtyRegion {
        DirtyRegion::None
    }

    /// Reset all hover state.  Called when navigating away from or to this
    /// page so stale highlights don't persist across navigation.
    fn reset_hover(&mut self) {}

    /// Returns a pending floating window to open, consuming the request.
    ///
    /// Called by `Application` immediately after `on_mouse_input`.  The default
    /// implementation returns `None`; pages override this to open modal overlays.
    fn take_open_request(&mut self) -> Option<Box<dyn crate::ui::floating_window::FloatingWindow>> {
        None
    }

    /// Called every frame when there is active animation.
    fn tick_animation(&mut self, _width: f32, _height: f32, _plan: &Plan) -> DirtyRegion {
        DirtyRegion::None
    }

    /// Returns true if this page has active animation that needs per-frame updates.
    fn has_animation(&self) -> bool {
        false
    }
}

/// Owns all page instances and tracks the currently active one.
///
/// The active page receives input events and is rendered each frame.
pub struct PageManager {
    pub active: PageId,
    pub home: home::HomePage,
    pub daily: daily::DailyPage,
    pub overview: overview::OverviewPage,
    pub settings: settings::SettingsPage,
    pub allocation: allocation::AllocationPage,
    pub calendar_overrides: calendar_overrides::CalendarOverridesPage,
}

impl PageManager {
    pub fn new() -> Self {
        Self {
            active: PageId::Home,
            home: home::HomePage::new(),
            daily: daily::DailyPage::new(),
            overview: overview::OverviewPage::new(),
            settings: settings::SettingsPage::new(),
            allocation: allocation::AllocationPage::new(),
            calendar_overrides: calendar_overrides::CalendarOverridesPage::new(),
        }
    }

    /// Returns a shared reference to the active page as a [`Page`] trait object.
    pub fn active_page(&self) -> &dyn Page {
        match self.active {
            PageId::Home => &self.home,
            PageId::Daily => &self.daily,
            PageId::Overview => &self.overview,
            PageId::Settings => &self.settings,
            PageId::Allocation => &self.allocation,
            PageId::CalendarOverrides => &self.calendar_overrides,
        }
    }

    /// Returns a mutable reference to the active page as a [`Page`] trait object.
    pub fn active_page_mut(&mut self) -> &mut dyn Page {
        match self.active {
            PageId::Home => &mut self.home,
            PageId::Daily => &mut self.daily,
            PageId::Overview => &mut self.overview,
            PageId::Settings => &mut self.settings,
            PageId::Allocation => &mut self.allocation,
            PageId::CalendarOverrides => &mut self.calendar_overrides,
        }
    }

    /// Changes the active page.  Does not reset page state.
    pub fn set_active(&mut self, page: PageId) {
        self.active = page;
    }

    /// Returns a mutable reference to the settings page.
    pub fn settings_mut(&mut self) -> &mut settings::SettingsPage {
        &mut self.settings
    }

    /// Returns a mutable reference to the calendar overrides page.
    pub fn calendar_overrides_mut(&mut self) -> &mut calendar_overrides::CalendarOverridesPage {
        &mut self.calendar_overrides
    }
}
