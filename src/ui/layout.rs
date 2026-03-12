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
pub const BACK_BTN_CORNER: f32 = 4.0;
pub const BACK_BTN_HOVER_BG: u32 = 0xff_e8e8e8;
pub const BACK_BTN_ICON_COLOR: u32 = 0xff_555555;

// Page-specific toolbar icon buttons (sit to the right of the back button)
pub const TOOLBAR_BTN_GAP: f32 = 8.0;
pub const TOOLBAR_BTN_Y: f32 = BACK_BTN_Y;
pub const TOOLBAR_BTN_SIZE: f32 = BACK_BTN_SIZE;
pub const TOOLBAR_BTN_CORNER: f32 = BACK_BTN_CORNER;
pub const TOOLBAR_BTN_ICON_COLOR: u32 = 0xff_555555;
pub const TOOLBAR_BTN_HOVER_BG: u32 = 0xff_e8e8e8;
/// Stroke width for toolbar button icons, proportional to button size.
/// At the default 36 px button this equals 2.0 px.
pub const TOOLBAR_STROKE_WIDTH: f32 = BACK_BTN_SIZE / 18.0;
/// X position of the Nth page toolbar button (0-indexed).
pub const fn toolbar_btn_x(n: u32) -> f32 {
    BACK_BTN_X + BACK_BTN_SIZE + TOOLBAR_BTN_GAP + n as f32 * (TOOLBAR_BTN_SIZE + TOOLBAR_BTN_GAP)
}

// Planning page list panel
pub const PLAN_LIST_ITEM_H: f32 = 36.0;
pub const PLAN_LIST_PADDING: f32 = 8.0;
pub const PLAN_LIST_SECTION_H: f32 = 28.0;
pub const PLAN_ADD_BTN_H: f32 = 30.0;

// Planning page form panel
pub const PLAN_FORM_PADDING: f32 = 20.0;
pub const PLAN_INPUT_H: f32 = 32.0;
pub const PLAN_LABEL_GAP: f32 = 6.0;
pub const PLAN_FIELD_GAP: f32 = 16.0;
pub const PLAN_BTN_H: f32 = 30.0;
pub const PLAN_BTN_CORNER: f32 = 4.0;

// Colors
pub const LIST_BG: u32 = 0xff_f7f7f7;
pub const LIST_ITEM_HOVER_BG: u32 = 0xff_efefef;
pub const LIST_ITEM_SEL_BG: u32 = 0xff_ddeeff;
pub const LIST_SECTION_FG: u32 = 0xff_aaaaaa;
pub const ADD_BTN_BG: u32 = 0xff_f0f0f0;
pub const ADD_BTN_HOVER_BG: u32 = 0xff_e0e0e0;
pub const ADD_BTN_FG: u32 = 0xff_555555;
pub const INPUT_BG: u32 = 0xff_ffffff;
pub const INPUT_BORDER: u32 = 0xff_cccccc;
pub const INPUT_BORDER_ERROR: u32 = 0xff_e5_39_35;
pub const INPUT_BORDER_FOCUS: u32 = 0xff_4a90d9;
pub const INPUT_FG: u32 = 0xff_222222;
pub const INPUT_CURSOR_COLOR: u32 = 0xff_4a90d9;
pub const LABEL_FG: u32 = 0xff_666666;
pub const BTN_PRIMARY_BG: u32 = 0xff_4a90d9;
pub const BTN_PRIMARY_FG: u32 = 0xff_ffffff;
pub const BTN_SECONDARY_BG: u32 = 0xff_f0f0f0;
pub const BTN_SECONDARY_FG: u32 = 0xff_333333;
pub const BTN_DANGER_BG: u32 = 0xff_e53935;
pub const BTN_DANGER_FG: u32 = 0xff_ffffff;
pub const ITEM_FG: u32 = 0xff_222222;
pub const ITEM_TASK_DOT: u32 = 0xff_4a90d9;
pub const ITEM_MILESTONE_DOT: u32 = 0xff_f5a623;
