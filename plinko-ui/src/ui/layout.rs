//! Layout constants and colour palette used by all page renderers.
//!
//! Colours are stored as `0xAA_RRGGBB` `u32` values and converted to
//! [`skia_safe::Color`] via `Color::from(value)`.

// Layout constants
pub const DIVIDER_WIDTH: f32 = 6.0;

// Colors (used by page renders)
pub const PANEL_BG: u32 = 0xff_252526;
pub const PANEL_TEXT: u32 = 0xff_888888;
pub const DIVIDER_COLOR: u32 = 0xff_3d3d3d;
pub const DIVIDER_ACTIVE_COLOR: u32 = 0xff_6a6a6a;
pub const DIVIDER_GRIP_COLOR: u32 = 0xff_4a4a4a;
pub const DIVIDER_GRIP_ACTIVE_COLOR: u32 = 0xff_888888;

// Home page
pub const HOME_CARD_SIZE: f32 = 160.0;
pub const HOME_CARD_GAP: f32 = 32.0;
pub const HOME_CARD_CORNER: f32 = 12.0;
pub const HOME_CARD_ICON_SIZE: f32 = 48.0;
pub const HOME_BG: u32 = 0xff_1e1e1e;
pub const HOME_CARD_BG: u32 = 0xff_252526;
pub const HOME_CARD_HOVER_BG: u32 = 0xff_2d2d30;
pub const HOME_CARD_BORDER: u32 = 0xff_3d3d3d;
pub const HOME_CARD_LABEL_COLOR: u32 = 0xff_d4d4d4;
pub const HOME_ICON_COLOR: u32 = 0xff_a0a0a0;

// Back button
pub const BACK_BTN_X: f32 = 16.0;
pub const BACK_BTN_Y: f32 = 16.0;
pub const BACK_BTN_SIZE: f32 = 36.0;
pub const BACK_BTN_CORNER: f32 = 4.0;
pub const BACK_BTN_HOVER_BG: u32 = 0xff_2d2d30;
pub const BACK_BTN_ICON_COLOR: u32 = 0xff_a0a0a0;

// Page-specific toolbar icon buttons (sit to the right of the back button)
pub const TOOLBAR_BTN_GAP: f32 = 8.0;
pub const TOOLBAR_BTN_Y: f32 = BACK_BTN_Y;
pub const TOOLBAR_BTN_SIZE: f32 = BACK_BTN_SIZE;
pub const TOOLBAR_BTN_CORNER: f32 = BACK_BTN_CORNER;
pub const TOOLBAR_BTN_ICON_COLOR: u32 = 0xff_a0a0a0;
pub const TOOLBAR_BTN_HOVER_BG: u32 = 0xff_2d2d30;
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
pub const LIST_BG: u32 = 0xff_252526;
pub const LIST_ITEM_HOVER_BG: u32 = 0xff_2d2d30;
pub const LIST_ITEM_SEL_BG: u32 = 0xff_1a3a5c;
pub const LIST_SECTION_FG: u32 = 0xff_707070;
pub const ADD_BTN_BG: u32 = 0xff_2d2d30;
pub const ADD_BTN_HOVER_BG: u32 = 0xff_3d3d40;
pub const ADD_BTN_FG: u32 = 0xff_a0a0a0;
pub const INPUT_BG: u32 = 0xff_1e1e1e;
pub const INPUT_BORDER: u32 = 0xff_4a4a4a;
pub const INPUT_BORDER_ERROR: u32 = 0xff_e5_39_35;
pub const INPUT_BORDER_FOCUS: u32 = 0xff_4a90d9;
pub const INPUT_FG: u32 = 0xff_d4d4d4;
pub const INPUT_CURSOR_COLOR: u32 = 0xff_4a90d9;
pub const LABEL_FG: u32 = 0xff_8a8a8a;
pub const BTN_PRIMARY_BG: u32 = 0xff_4a90d9;
pub const BTN_PRIMARY_FG: u32 = 0xff_ffffff;
pub const BTN_SECONDARY_BG: u32 = 0xff_2d2d30;
pub const BTN_SECONDARY_FG: u32 = 0xff_d4d4d4;
pub const BTN_DANGER_BG: u32 = 0xff_e53935;
pub const BTN_DANGER_HOVER_BG: u32 = 0xff_c62828;
pub const BTN_DANGER_FG: u32 = 0xff_ffffff;
pub const ITEM_FG: u32 = 0xff_d4d4d4;
pub const ITEM_TASK_DOT: u32 = 0xff_4a90d9;
pub const ITEM_MILESTONE_DOT: u32 = 0xff_f5a623;
pub const DEP_PLAN_START_FG: u32 = 0xff_00897b; // Teal — Plan Start special node

// Extended colour palette
pub const MUTED_FG: u32 = 0xff_8a8a8a;
pub const PLACEHOLDER_FG: u32 = 0xff_606060;
pub const SUBTLE_FG: u32 = 0xff_707070;
pub const GHOST_FG: u32 = 0xff_505050;
pub const SUBTLE_BG: u32 = 0xff_252526;
pub const BTN_PRIMARY_HOVER_BG: u32 = 0xff_3a7bc8;
pub const CAL_SELECTED_BG: u32 = 0xff_1a3a5c;
pub const ERROR_BG: u32 = 0xff_3d1a1a;
pub const ICON_DELETE_COLOR: u32 = 0xff_e05555;
pub const LINK_COLOR: u32 = 0xff_4da6ff;
pub const SCROLLBAR_THUMB_COLOR: u32 = 0x50_ffffff;
pub const OVERLAY_XLIGHT: u32 = 0x1e_000000;
pub const OVERLAY_LIGHT: u32 = 0x23_000000;
pub const OVERLAY_SOFT: u32 = 0x28_000000;
pub const OVERLAY_MEDIUM: u32 = 0x64_000000;
pub const OVERLAY_DARK: u32 = 0x78_000000;
pub const TOOLTIP_BG: u32 = 0xdc_1e1e1e;
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
pub const GANTT_BG: u32 = 0xff_1e1e1e;
pub const GANTT_HEADER_BG: u32 = 0xff_252526;
pub const GANTT_HEADER_BORDER: u32 = 0xff_3d3d3d;
pub const GANTT_HEADER_FG: u32 = 0xff_a0a0a0;
pub const GANTT_HEADER_MONTH_FG: u32 = 0xff_d4d4d4;
pub const GANTT_DAY_LINE_COLOR: u32 = 0xff_3d3d3d;
pub const GANTT_TODAY_LINE_COLOR: u32 = 0x80_4a90d9;
pub const GANTT_ROW_ALT_BG: u32 = 0xff_232326;
pub const GANTT_WEEKEND_BG: u32 = 0xff_1a1a20;

// Task status colors
pub const GANTT_TASK_NOT_STARTED: u32 = 0xff_555555;
pub const GANTT_TASK_IN_PROGRESS: u32 = 0xff_f5a623;
pub const GANTT_TASK_ON_HOLD: u32 = 0xff_b39ddb;
pub const GANTT_TASK_COMPLETE: u32 = 0xff_66bb6a;
pub const GANTT_TASK_DROPPED: u32 = 0xff_757575;

// Task label colors (on bars)
pub const GANTT_TASK_LABEL_DARK: u32 = 0xff_333333; // on light bars
pub const GANTT_TASK_LABEL_LIGHT: u32 = 0xff_ffffff; // on dark bars

// Milestone status colors
pub const GANTT_MS_NOT_STARTED: u32 = 0xff_888888;
pub const GANTT_MS_IN_PROGRESS: u32 = 0xff_f5a623;
pub const GANTT_MS_COMPLETE: u32 = 0xff_66bb6a;
/// Teal colour used for the Plan Start fixed marker on the Gantt chart.
pub const GANTT_PLAN_START_COLOR: u32 = DEP_PLAN_START_FG;

/// Indigo flag drawn on tasks/milestones that have no dependents (end nodes).
pub const GANTT_END_NODE_COLOR: u32 = 0xff_5c6bc0;

// Dependency line color
pub const GANTT_DEP_LINE_COLOR: u32 = 0x80_888888;
/// Dimmed dep line when another node is hovered.
pub const GANTT_DEP_LINE_DIMMED: u32 = 0x20_888888;

// Hover/dependency highlight colors (overview Gantt)
pub const HOVER_SELF_GLOW: u32 = 0xff_1e88e5;
pub const HOVER_SELF_BORDER: u32 = 0xff_1e88e5;
pub const HOVER_UPSTREAM_GLOW: u32 = 0xff_fc1ef1; // light purple
pub const HOVER_UPSTREAM_BORDER: u32 = 0xff_fc1ef1;
pub const HOVER_ARROW_UPSTREAM_GLOW: u32 = 0x25_fc1ef1;
pub const HOVER_ARROW_UPSTREAM: u32 = 0xa8_fc1ef1;
pub const HOVER_DOWNSTREAM_GLOW: u32 = 0xff_07fcd7; // light teal/cyan
pub const HOVER_DOWNSTREAM_BORDER: u32 = 0xff_07fcd7;
pub const HOVER_ARROW_DOWNSTREAM_GLOW: u32 = 0x25_07fcd7;
pub const HOVER_ARROW_DOWNSTREAM: u32 = 0xa8_07fcd7;

// Plan-target highlight (overview Gantt)
pub const TARGET_GLOW: u32 = 0x80_ffd600; // semi-transparent gold glow
pub const TARGET_BORDER: u32 = 0xff_ffd600; // solid gold border

// Warning / constraint violation icon (overview Gantt)
pub const WARN_FILL: u32 = 0xff_ffc107; // amber triangle fill
pub const WARN_STROKE: u32 = 0xff_e65100; // amber triangle outline
pub const WARN_TOOLTIP_BG: u32 = 0xf0_333333;
pub const WARN_TOOLTIP_FG: u32 = 0xff_ffffff;
pub const WARN_ICON_GLYPH: u32 = 0xcc_000000; // dark glyph drawn on amber triangle

// Allocation page — user panel (left column)
pub const ALLOC_USER_LABEL_W: f32 = 120.0; // kept for unused-const compat
pub const ALLOC_TASK_LABEL_W: f32 = 140.0;
pub const ALLOC_USER_PANEL_W: f32 = 180.0;
pub const ALLOC_USER_ENTRY_H: f32 = 52.0;
pub const ALLOC_UTIL_ROW_H: f32 = 28.0;
pub const ALLOC_ROW_ALT_BG: u32 = 0xff_232326;
pub const ALLOC_SELECTED_BG: u32 = 0xff_1a3a5c;
pub const ALLOC_HOVER_BG: u32 = 0xff_1e2a40;
pub const ALLOC_UTIL_GREEN: u32 = 0xff_4caf50;
pub const ALLOC_UTIL_AMBER: u32 = 0xff_ff9800;
pub const ALLOC_UTIL_RED: u32 = 0xff_f44336;
/// Overflow cap indicator — stacked on top of a full bar.
pub const ALLOC_OVERFLOW_COLOR: u32 = 0xff_cc0000;
/// Today marker line on the allocation page (more opaque than Gantt today line).
pub const ALLOC_TODAY_LINE_COLOR: u32 = 0xcc_4a90d9;
/// Weekend column header text in the allocation timeline.
pub const ALLOC_WEEKEND_HEADER_FG: u32 = 0xff_aaaaaa;

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
pub const CAL_HOLIDAY_BG: u32 = 0xff_3d1a1a; // holiday (0h)
pub const CAL_PARTIAL_BG: u32 = 0xff_3d3010; // partial-day override
pub const CAL_NONWORK_BG: u32 = 0xff_1a1a1a; // non-working day (no override)
pub const CAL_WORK_BG: u32 = 0xff_252526; // normal working day
pub const CAL_HOVER_BG: u32 = 0xff_1a2d40; // hovered day cell
pub const CAL_OUTSIDE_BG: u32 = 0xff_141414; // outside current month
pub const CAL_TODAY_BORDER: u32 = 0xff_4a90d9; // today highlight border
pub const CAL_CELL_BORDER: u32 = 0xff_333333; // cell border
pub const CAL_HEADER_BG: u32 = 0xff_252526; // day-of-week header bg
pub const CAL_FG: u32 = 0xff_d4d4d4; // cell text
pub const CAL_DIM_FG: u32 = 0xff_505050; // outside month text
/// Hovered state for calendar-override day-type toggle buttons.
pub const CAL_BTN_HOVER_BG: u32 = 0xff_3a3a3a;
/// Normal (un-hovered, unselected) state for calendar-override toggle buttons.
pub const CAL_BTN_NORMAL_BG: u32 = 0xff_2a2a2a;
/// Delete / remove action color in the calendar overrides page.
pub const CAL_DELETE_COLOR: u32 = 0xff_cc3333;

/// Generic panel drop-shadow overlay.
pub const SHADOW_COLOR: u32 = 0x30_000000;
