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

/// X position of the settings (cogwheel) button on the right side of the toolbar.
pub fn settings_btn_x(window_width: f32) -> f32 {
    window_width - TOOLBAR_BTN_SIZE - TOOLBAR_BTN_GAP
}

/// X position of the person button when placed to the left of settings.
pub fn person_right_btn_x(window_width: f32) -> f32 {
    window_width - 2.0 * (TOOLBAR_BTN_SIZE + TOOLBAR_BTN_GAP)
}

/// X position of the rightmost settings button.
pub fn settings_right_btn_x(window_width: f32) -> f32 {
    window_width - (TOOLBAR_BTN_SIZE + TOOLBAR_BTN_GAP)
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
pub const DEP_PLAN_START_FG: u32 = 0xff_00897b; // Teal — Plan Start special node

// Extended colour palette
pub const MUTED_FG: u32 = 0xff_aaaaaa;
pub const PLACEHOLDER_FG: u32 = 0xff_888888;
pub const SUBTLE_FG: u32 = 0xff_999999;
pub const GHOST_FG: u32 = 0xff_bbbbbb;
pub const SUBTLE_BG: u32 = 0xff_f5f5f5;
pub const BTN_PRIMARY_HOVER_BG: u32 = 0xff_3a7bc8;
pub const CAL_SELECTED_BG: u32 = 0xff_e8eef8;
pub const ERROR_BG: u32 = 0xff_ffeeee;
pub const ICON_DELETE_COLOR: u32 = 0xff_cc2222;
pub const LINK_COLOR: u32 = 0xff_2196f3;
pub const SCROLLBAR_THUMB_COLOR: u32 = 0x50_000000;
pub const OVERLAY_XLIGHT: u32 = 0x1e_000000;
pub const OVERLAY_LIGHT: u32 = 0x23_000000;
pub const OVERLAY_SOFT: u32 = 0x28_000000;
pub const OVERLAY_MEDIUM: u32 = 0x64_000000;
pub const OVERLAY_DARK: u32 = 0x78_000000;
pub const TOOLTIP_BG: u32 = 0xdc_ffffff;
// ── Gantt chart ────────────────────────────────────────────────────────────────

// Layout
pub const GANTT_MONTH_ROW_H: f32 = 18.0;
pub const GANTT_DAY_ROW_H: f32 = 28.0;
pub const GANTT_HEADER_H: f32 = GANTT_MONTH_ROW_H + GANTT_DAY_ROW_H;
pub const GANTT_ROW_H: f32 = 36.0;
pub const GANTT_ROW_PADDING: f32 = 5.0;
pub const GANTT_BAR_CORNER: f32 = 4.0;
pub const GANTT_DAY_LINE_W: f32 = 6.0;
pub const GANTT_ZOOM_DEFAULT: f32 = 40.0;
pub const GANTT_ZOOM_MIN: f32 = 8.0;
pub const GANTT_ZOOM_MAX: f32 = 200.0;
pub const GANTT_MS_HALF: f32 = 10.0; // milestone diamond half-size

// Chrome
pub const GANTT_BG: u32 = 0xff_fafafa;
pub const GANTT_HEADER_BG: u32 = 0xff_f0f0f0;
pub const GANTT_HEADER_BORDER: u32 = 0xff_d8d8d8;
pub const GANTT_HEADER_FG: u32 = 0xff_555555;
pub const GANTT_HEADER_MONTH_FG: u32 = 0xff_333333;
pub const GANTT_DAY_LINE_COLOR: u32 = 0xff_e4e4e4;
pub const GANTT_TODAY_LINE_COLOR: u32 = 0x80_4a90d9;
pub const GANTT_ROW_ALT_BG: u32 = 0xff_f4f4f4;
pub const GANTT_WEEKEND_BG: u32 = 0xff_efefef;

// Task status colors
pub const GANTT_TASK_NOT_STARTED: u32 = 0xff_d0d0d0;
pub const GANTT_TASK_IN_PROGRESS: u32 = 0xff_f5a623;
pub const GANTT_TASK_ON_HOLD: u32 = 0xff_b39ddb;
pub const GANTT_TASK_COMPLETE: u32 = 0xff_66bb6a;
pub const GANTT_TASK_DROPPED: u32 = 0xff_757575;

// Task label colors (on bars)
pub const GANTT_TASK_LABEL_DARK: u32 = 0xff_333333; // on light bars
pub const GANTT_TASK_LABEL_LIGHT: u32 = 0xff_ffffff; // on dark bars

// Milestone status colors
pub const GANTT_MS_NOT_STARTED: u32 = 0xff_bdbdbd;
pub const GANTT_MS_IN_PROGRESS: u32 = 0xff_f5a623;
pub const GANTT_MS_COMPLETE: u32 = 0xff_66bb6a;
/// Teal colour used for the Plan Start fixed marker on the Gantt chart.
pub const GANTT_PLAN_START_COLOR: u32 = DEP_PLAN_START_FG;

/// Indigo flag drawn on tasks/milestones that have no dependents (end nodes).
pub const GANTT_END_NODE_COLOR: u32 = 0xff_5c6bc0;

// Dependency line color
pub const GANTT_DEP_LINE_COLOR: u32 = 0x80_888888;

// Allocation page — user panel (left column)
pub const ALLOC_USER_LABEL_W: f32 = 120.0; // kept for unused-const compat
pub const ALLOC_TASK_LABEL_W: f32 = 140.0;
pub const ALLOC_USER_PANEL_W: f32 = 180.0;
pub const ALLOC_USER_ENTRY_H: f32 = 52.0;
pub const ALLOC_UTIL_ROW_H: f32 = 28.0;
pub const ALLOC_ROW_ALT_BG: u32 = 0xff_f7f7f7;
pub const ALLOC_SELECTED_BG: u32 = 0xff_dde9fb;
pub const ALLOC_HOVER_BG: u32 = 0xff_f0f4ff;
pub const ALLOC_UTIL_GREEN: u32 = 0xff_4caf50;
pub const ALLOC_UTIL_AMBER: u32 = 0xff_ff9800;
pub const ALLOC_UTIL_RED: u32 = 0xff_f44336;

// Task color palette for allocation bars
pub const TASK_COLORS: [u32; 10] = [
    0xff_4a90d9, // blue
    0xff_7ed321, // green
    0xff_f5a623, // amber
    0xff_d0021b, // red
    0xff_9b59b6, // purple
    0xff_1abc9c, // teal
    0xff_e67e22, // orange
    0xff_2ecc71, // mint
    0xff_e74c3c, // coral
    0xff_3498db, // sky blue
];

// Calendar overrides page
pub const CAL_HOLIDAY_BG: u32 = 0xff_ffcccc; // holiday (0h)
pub const CAL_PARTIAL_BG: u32 = 0xff_fff3cd; // partial-day override
pub const CAL_NONWORK_BG: u32 = 0xff_f0f0f0; // non-working day (no override)
pub const CAL_WORK_BG: u32 = 0xff_ffffff; // normal working day
pub const CAL_HOVER_BG: u32 = 0xff_e8f4ff; // hovered day cell
pub const CAL_OUTSIDE_BG: u32 = 0xff_fafafa; // outside current month
pub const CAL_TODAY_BORDER: u32 = 0xff_4a90d9; // today highlight border
pub const CAL_CELL_BORDER: u32 = 0xff_e0e0e0; // cell border
pub const CAL_HEADER_BG: u32 = 0xff_f5f5f5; // day-of-week header bg
pub const CAL_FG: u32 = 0xff_333333; // cell text
pub const CAL_DIM_FG: u32 = 0xff_aaaaaa; // outside month text
