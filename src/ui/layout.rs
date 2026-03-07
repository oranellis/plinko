//! Layout constants and colour palette used by all page renderers.
//!
//! Colours are stored as `0xAA_RRGGBB` `u32` values and converted to
//! [`skia_safe::Color`] via `Color::from(value)`.

// Layout constants
pub const DIVIDER_WIDTH: f32 = 6.0;

// Colors (used by page renders)
pub const PANEL_BG: u32 = 0xff_ffffff;
pub const PANEL_TEXT: u32 = 0xff_888888;
pub const DIVIDER_COLOR: u32 = 0xff_e0e0e0;
pub const DIVIDER_ACTIVE_COLOR: u32 = 0xff_aaaaaa;
pub const DIVIDER_GRIP_COLOR: u32 = 0xff_999999;
pub const DIVIDER_GRIP_ACTIVE_COLOR: u32 = 0xff_666666;

// Home page
pub const HOME_CARD_SIZE: f32 = 160.0;
pub const HOME_CARD_GAP: f32 = 32.0;
pub const HOME_CARD_CORNER: f32 = 12.0;
pub const HOME_CARD_ICON_SIZE: f32 = 48.0;
pub const HOME_BG: u32 = 0xff_f5f5f5;
pub const HOME_CARD_BG: u32 = 0xff_ffffff;
pub const HOME_CARD_HOVER_BG: u32 = 0xff_e8e8e8;
pub const HOME_CARD_BORDER: u32 = 0xff_e0e0e0;
pub const HOME_CARD_LABEL_COLOR: u32 = 0xff_333333;
pub const HOME_ICON_COLOR: u32 = 0xff_555555;

// Back button
pub const BACK_BTN_X: f32 = 16.0;
pub const BACK_BTN_Y: f32 = 16.0;
pub const BACK_BTN_SIZE: f32 = 36.0;
pub const BACK_BTN_CORNER: f32 = 6.0;
pub const BACK_BTN_HOVER_BG: u32 = 0xff_e8e8e8;
pub const BACK_BTN_ICON_COLOR: u32 = 0xff_555555;
