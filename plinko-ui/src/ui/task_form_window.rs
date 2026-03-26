//! Floating form for creating or editing a task — all task fields.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::event::Modifiers;
use winit::keyboard::{Key, NamedKey};

use crate::engine::PlanRequestSender;
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_SIZE, BTN_DANGER_BG, BTN_DANGER_FG, BTN_DANGER_HOVER_BG, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_PRIMARY_HOVER_BG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, CAL_SELECTED_BG,
    DEP_PLAN_START_FG, DIVIDER_COLOR, ERROR_BG, GHOST_FG, ICON_DELETE_COLOR, INPUT_BG,
    INPUT_BORDER, INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG,
    LABEL_FG, LINK_COLOR, LIST_BG, LIST_ITEM_HOVER_BG, MUTED_FG, OVERLAY_DARK, OVERLAY_LIGHT,
    OVERLAY_SOFT, OVERLAY_XLIGHT, PANEL_BG, PLACEHOLDER_FG, PLAN_BTN_CORNER, PLAN_BTN_H,
    PLAN_FIELD_GAP, PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP, SCROLLBAR_THUMB_COLOR,
    SUBTLE_BG, SUBTLE_FG, TOOLBAR_BTN_HOVER_BG, TOOLBAR_BTN_ICON_COLOR,
};
use crate::ui::multi_line_input::MultiLineInput;
use crate::ui::text_input::TextInput;
use plinko_shared::data::constraint::{ConstraintKind, DateConstraint};
use plinko_shared::data::dependency::Dependency;
use plinko_shared::data::ids::{MilestoneId, NodeId, TagId, UserId};
use plinko_shared::data::task::{Task, WorkerSlot};
use plinko_shared::data::{Plan, Status, TaskId};
use plinko_shared::protocol::{PlanRequest, TaskPatch, apply_task_patch};
use std::cell::Cell;
use std::collections::HashSet;

// ── Layout constants ──────────────────────────────────────────────────────────

const PANEL_W: f32 = 480.0;
const TITLE_H: f32 = 48.0;
const CORNER: f32 = 8.0;
const BTN_INSET: f32 = (TITLE_H - BACK_BTN_SIZE) / 2.0;
const LABEL_H: f32 = 14.0;
const FIELD_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + PLAN_INPUT_H;
const COL_GAP: f32 = 12.0;
const SAVE_BTN_W: f32 = 80.0;
const DELETE_BTN_W: f32 = 80.0;

// Multi-line description box
const DESC_LINE_H: f32 = 18.0;
const DESC_LINES: usize = 8;
const DESC_H: f32 = DESC_LINE_H * DESC_LINES as f32 + 8.0;
const DESC_BLOCK_H: f32 = LABEL_H + PLAN_LABEL_GAP + DESC_H;

// Row indices
const ROW_NAME: usize = 0;
const ROW_DESC: usize = 1;
const ROW_STATUS: usize = 2;
const ROW_DURATION: usize = 3;
const ROW_CONSTRAINT: usize = 4;
const ROW_DATES: usize = 5;

// Worker slots section
const WORKER_ROW_H: f32 = 36.0;
const WORKER_INPUT_H: f32 = 28.0;
const WORKER_WORKLOAD_W: f32 = 64.0;
const WORKER_REMOVE_SIZE: f32 = 22.0;
const WORKER_COL_GAP: f32 = 8.0;
const SLOT_TYPE_W: f32 = 50.0;
const WORKER_PAD_L: f32 = 4.0; // gap before T/P toggle
const WORKER_PAD_R: f32 = 8.0; // gap after X button (accommodates scrollbar)
const MAX_VISIBLE_WORKERS: usize = 3;
const PLUS_BTN_H: f32 = 28.0;
const WORKER_SECTION_H: f32 = LABEL_H + PLAN_LABEL_GAP + WORKER_ROW_H * 3.0 + PLUS_BTN_H;

// User picker dropdown inside a slot
const USER_DROPDOWN_FILTER_H: f32 = WORKER_INPUT_H;
const USER_DROPDOWN_ROW_H: f32 = 28.0;
const MAX_USER_DROPDOWN_ROWS: usize = 4;
const USER_DROPDOWN_H: f32 =
    USER_DROPDOWN_FILTER_H + MAX_USER_DROPDOWN_ROWS as f32 * USER_DROPDOWN_ROW_H;

// Dependency section
const DEP_ROW_H: f32 = 36.0;
const DEP_INPUT_H: f32 = 28.0;
const DEP_LAG_W: f32 = 64.0;
const DEP_REMOVE_SIZE: f32 = 22.0;
const DEP_COL_GAP: f32 = 8.0;
const DEP_PAD_L: f32 = 4.0;
const DEP_PAD_R: f32 = 8.0;
const MAX_VISIBLE_DEPS: usize = 3;
const DEP_SECTION_H: f32 =
    LABEL_H + PLAN_LABEL_GAP + DEP_ROW_H * MAX_VISIBLE_DEPS as f32 + PLUS_BTN_H;

// Dependency dropdown
const DEP_DROPDOWN_FILTER_H: f32 = DEP_INPUT_H;
const DEP_DROPDOWN_ROW_H: f32 = 28.0;
const MAX_DEP_DROPDOWN_ROWS: usize = 5;
const DEP_DROPDOWN_H: f32 =
    DEP_DROPDOWN_FILTER_H + MAX_DEP_DROPDOWN_ROWS as f32 * DEP_DROPDOWN_ROW_H;

// Forward dependents section (read-only, edit mode only)
const FWD_ROW_H: f32 = 28.0;
const FWD_MAX_ROWS: usize = 3;
const FWD_SECTION_H: f32 = LABEL_H + PLAN_LABEL_GAP + FWD_ROW_H * FWD_MAX_ROWS as f32;

// Calendar popup
const CAL_PAD: f32 = 8.0;
const CAL_CELL: f32 = 32.0;
const CAL_W: f32 = CAL_CELL * 7.0 + CAL_PAD * 2.0;
const CAL_HEADER_H: f32 = 28.0;
const CAL_DOW_H: f32 = 20.0;
const CAL_ROW_H: f32 = 26.0;
const CAL_FOOTER_H: f32 = 28.0;
const CAL_H: f32 = CAL_PAD + CAL_HEADER_H + CAL_DOW_H + CAL_ROW_H * 6.0 + CAL_FOOTER_H + CAL_PAD;

const PANEL_H: f32 = TITLE_H
    + 1.0
    + PLAN_FORM_PADDING
    + FIELD_BLOCK_H   // name
    + PLAN_FIELD_GAP
    + DESC_BLOCK_H    // description (tall)
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // status
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // duration
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // constraint kind + date
    + PLAN_FIELD_GAP
    + FIELD_BLOCK_H   // actual start + actual end
    + PLAN_FIELD_GAP
    + WORKER_SECTION_H
    + PLAN_FIELD_GAP
    + DEP_SECTION_H
    + PLAN_FIELD_GAP
    + FWD_SECTION_H   // forward dependents (edit mode)
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;

const SCROLLBAR_W: f32 = 4.0;

// ── Helper types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum TextField {
    None,
    Name,
    Description,
    Duration,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum OpenCalendar {
    Constraint,
    ActualStart,
    ActualEnd,
}

#[derive(Clone, Copy, PartialEq)]
enum ConstraintSel {
    None,
    Earliest,
    Fixed,
    Latest,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl ConstraintSel {
    fn from_opt(c: Option<DateConstraint>) -> (Self, Option<NaiveDate>) {
        match c {
            None => (Self::None, None),
            Some(dc) => {
                let sel = match dc.kind {
                    ConstraintKind::Earliest => Self::Earliest,
                    ConstraintKind::Fixed => Self::Fixed,
                    ConstraintKind::Latest => Self::Latest,
                };
                (sel, Some(dc.date))
            }
        }
    }

    fn to_constraint(self, date: Option<NaiveDate>) -> Option<DateConstraint> {
        let d = date?;
        match self {
            Self::None => None,
            Self::Earliest => Some(DateConstraint::earliest(d)),
            Self::Fixed => Some(DateConstraint::fixed(d)),
            Self::Latest => Some(DateConstraint::latest(d)),
        }
    }
}
// }}}

// ── CalendarPicker ────────────────────────────────────────────────────────────

struct CalendarPicker {
    value: Option<NaiveDate>,
    nav_year: i32,
    nav_month: u32,
    hovered_day: Option<u32>,
    hovered_prev_year: bool,
    hovered_prev_month: bool,
    hovered_next_month: bool,
    hovered_next_year: bool,
    hovered_clear: bool,
    hovered_today: bool,
    hovered_trigger: bool,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl CalendarPicker {
    fn new(value: Option<NaiveDate>) -> Self {
        let base = value.unwrap_or_else(|| chrono::Local::now().date_naive());
        Self {
            value,
            nav_year: base.year(),
            nav_month: base.month(),
            hovered_day: None,
            hovered_prev_year: false,
            hovered_prev_month: false,
            hovered_next_month: false,
            hovered_next_year: false,
            hovered_clear: false,
            hovered_today: false,
            hovered_trigger: false,
        }
    }

    fn prev_month(&mut self) {
        if self.nav_month == 1 {
            self.nav_month = 12;
            self.nav_year -= 1;
        } else {
            self.nav_month -= 1;
        }
    }

    fn next_month(&mut self) {
        if self.nav_month == 12 {
            self.nav_month = 1;
            self.nav_year += 1;
        } else {
            self.nav_month += 1;
        }
    }

    fn prev_year(&mut self) {
        self.nav_year -= 1;
    }

    fn next_year(&mut self) {
        self.nav_year += 1;
    }

    fn reset_hover(&mut self) {
        self.hovered_day = None;
        self.hovered_prev_year = false;
        self.hovered_prev_month = false;
        self.hovered_next_month = false;
        self.hovered_next_year = false;
        self.hovered_clear = false;
        self.hovered_today = false;
        self.hovered_trigger = false;
    }

    fn display_text(&self) -> String {
        match self.value {
            Some(d) => format!("{}", d.format("%d %b %Y")),
            None => "—".to_string(),
        }
    }
}
// }}}

/// Remove transitively redundant dependencies from `deps`.
/// A dependency `d` is redundant if another dependency `j` already transitively
/// depends on `d` (i.e. `d` is reachable from `j` in the existing plan graph).
/// Lag values are preserved on kept dependencies.
fn simplify_dependencies(deps: Vec<Dependency>, plan: &Plan) -> Vec<Dependency> {
    deps.iter()
        .filter(|d| {
            !deps
                .iter()
                .any(|j| j.id != d.id && plan.has_dependency_path(j.id, d.id))
        })
        .cloned()
        .collect()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    let first = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    (next.unwrap() - first).num_days() as u32
}

fn first_weekday_offset(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap()
        .weekday()
        .num_days_from_monday()
}

// ── WorkerSlotEdit ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum SlotType {
    Specific,
    Placeholder,
}

struct WorkerSlotEdit {
    slot_type: SlotType,
    user_id: Option<UserId>,
    user_filter: TextInput,
    required_tags: HashSet<TagId>,
    tag_filter: TextInput,
    workload: TextInput,
    hovered_type: Option<usize>,
    hovered_user_btn: bool,
    hovered_remove: bool,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl WorkerSlotEdit {
    fn new() -> Self {
        Self {
            slot_type: SlotType::Placeholder,
            user_id: None,
            user_filter: TextInput::new(""),
            required_tags: HashSet::new(),
            tag_filter: TextInput::new(""),
            workload: TextInput::new("1"),
            hovered_type: None,
            hovered_user_btn: false,
            hovered_remove: false,
        }
    }

    fn from_slot(slot: &WorkerSlot) -> Self {
        match slot {
            WorkerSlot::Specific {
                user_id,
                workload_days,
            } => Self {
                slot_type: SlotType::Specific,
                user_id: Some(*user_id),
                user_filter: TextInput::new(""),
                required_tags: HashSet::new(),
                tag_filter: TextInput::new(""),
                workload: TextInput::new(format_days(*workload_days)),
                hovered_type: None,
                hovered_user_btn: false,
                hovered_remove: false,
            },
            WorkerSlot::Placeholder {
                required_tags,
                workload_days,
            } => Self {
                slot_type: SlotType::Placeholder,
                user_id: None,
                user_filter: TextInput::new(""),
                required_tags: required_tags.clone(),
                tag_filter: TextInput::new(""),
                workload: TextInput::new(format_days(*workload_days)),
                hovered_type: None,
                hovered_user_btn: false,
                hovered_remove: false,
            },
        }
    }

    fn is_complete(&self) -> bool {
        match self.slot_type {
            SlotType::Specific => self.user_id.is_some(),
            SlotType::Placeholder => true,
        }
    }

    fn to_worker_slot(&self) -> Option<WorkerSlot> {
        let days: f32 = self
            .workload
            .content
            .trim()
            .parse::<f32>()
            .unwrap_or(1.0)
            .max(0.0);
        match self.slot_type {
            SlotType::Specific => {
                let uid = self.user_id?;
                Some(WorkerSlot::Specific {
                    user_id: uid,
                    workload_days: days,
                })
            }
            SlotType::Placeholder => Some(WorkerSlot::Placeholder {
                required_tags: self.required_tags.clone(),
                workload_days: days,
            }),
        }
    }

    fn filtered_users<'a>(
        &self,
        plan: &'a Plan,
    ) -> Vec<(&'a UserId, &'a plinko_shared::data::User)> {
        let filter = self.user_filter.content.to_lowercase();
        plan.users_data
            .iter()
            .map(|(id, ud)| (id, &ud.user))
            .filter(|(_, u)| filter.is_empty() || u.name.to_lowercase().contains(filter.as_str()))
            .collect::<Vec<_>>()
            .tap_sort_by(|(_, a), (_, b)| a.name.cmp(&b.name))
    }

    fn filtered_tags<'a>(&self, plan: &'a Plan) -> Vec<&'a plinko_shared::data::Tag> {
        let filter = self.tag_filter.content.to_lowercase();
        plan.tags
            .iter()
            .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()))
            .collect()
    }
}
// }}}

trait TapSortBy<T> {
    fn tap_sort_by<F: FnMut(&T, &T) -> std::cmp::Ordering>(self, f: F) -> Self;
}

impl<T> TapSortBy<T> for Vec<T> {
    fn tap_sort_by<F: FnMut(&T, &T) -> std::cmp::Ordering>(mut self, mut f: F) -> Self {
        self.sort_by(|a, b| f(a, b));
        self
    }
}

fn format_days(v: f32) -> String {
    if v == v.floor() && v >= 0.0 {
        format!("{}", v as u32)
    } else {
        format!("{}", v)
    }
}

// ── DependencyEdit ────────────────────────────────────────────────────────────

struct DependencyEdit {
    target: Option<NodeId>,
    dep_filter: TextInput,
    lag_input: TextInput,
    hovered_target: bool,
    hovered_remove: bool,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl DependencyEdit {
    fn new() -> Self {
        Self {
            target: None,
            dep_filter: TextInput::new(""),
            lag_input: TextInput::new(""),
            hovered_target: false,
            hovered_remove: false,
        }
    }
}
// }}}

// ── Mode ──────────────────────────────────────────────────────────────────────

enum Mode {
    New,
    Edit(TaskId),
}

// ── Text utilities ────────────────────────────────────────────────────────────

/// Wraps `text` into lines that fit within `max_w` pixels using the given font.
fn wrap_text(text: &str, font: &skia_safe::Font, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        let (w, _) = font.measure_str(&candidate, None);
        if w > max_w && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct TaskFormWindow {
    mode: Mode,
    name: TextInput,
    description: MultiLineInput,
    duration: TextInput,
    focused: TextField,
    status: Status,
    hovered_status: Option<usize>,
    constraint_kind: ConstraintSel,
    hovered_constraint_kind: Option<usize>,
    constraint_date: CalendarPicker,
    actual_start: CalendarPicker,
    actual_end: CalendarPicker,
    open_calendar: Option<OpenCalendar>,
    // Workers
    workers: Vec<WorkerSlotEdit>,
    worker_scroll_y: f32,
    cursor_in_worker_list: bool,
    open_slot_dropdown: Option<usize>,
    slot_dropdown_hovered: Option<usize>,
    slot_dropdown_scroll: usize,
    focused_slot_workload: Option<usize>,
    hovered_plus: bool,
    worker_error: bool,
    // Dependencies
    dependencies: Vec<DependencyEdit>,
    dep_scroll_y: f32,
    cursor_in_dep_list: bool,
    dep_dropdown_open_for: Option<usize>,
    dep_dropdown_hovered: Option<usize>,
    dep_dropdown_scroll: usize,
    focused_dep_lag: Option<usize>,
    hovered_dep_plus: bool,
    dep_error: bool,
    name_error: bool,
    duration_error: bool,
    constraint_date_error: bool,
    actual_start_error: bool,
    actual_end_error: bool,
    relaxed_mode: bool,
    hovered_relaxed: bool,
    // Buttons
    hovered_back: bool,
    hovered_save: bool,
    hovered_delete: bool,
    // Scroll
    cursor_in_desc: bool,
    form_scroll_y: f32,
    /// Cached max scroll for the description box, updated each render frame.
    max_desc_scroll: Cell<f32>,
    /// Scheduler error from the last submit attempt; shown as a red banner.
    scheduler_error: Option<String>,
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl TaskFormWindow {
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            description: MultiLineInput::new(""),
            duration: TextInput::new(""),
            focused: TextField::Name,
            status: Status::NotStarted,
            hovered_status: None,
            constraint_kind: ConstraintSel::None,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(None),
            actual_start: CalendarPicker::new(None),
            actual_end: CalendarPicker::new(None),
            open_calendar: None,
            workers: Vec::new(),
            worker_scroll_y: 0.0,
            cursor_in_worker_list: false,
            open_slot_dropdown: None,
            slot_dropdown_hovered: None,
            slot_dropdown_scroll: 0,
            focused_slot_workload: None,
            hovered_plus: false,
            worker_error: false,
            dependencies: vec![DependencyEdit {
                target: Some(NodeId::PlanStart),
                ..DependencyEdit::new()
            }],
            dep_scroll_y: 0.0,
            cursor_in_dep_list: false,
            dep_dropdown_open_for: None,
            dep_dropdown_hovered: None,
            dep_dropdown_scroll: 0,
            focused_dep_lag: None,
            hovered_dep_plus: false,
            dep_error: false,
            name_error: false,
            duration_error: false,
            constraint_date_error: false,
            actual_start_error: false,
            hovered_back: false,
            actual_end_error: false,
            relaxed_mode: false,
            hovered_relaxed: false,
            hovered_save: false,
            hovered_delete: false,
            cursor_in_desc: false,
            form_scroll_y: 0.0,
            max_desc_scroll: Cell::new(0.0),
            scheduler_error: None,
        }
    }

    pub fn from_task(task: &Task, plan: &plinko_shared::data::Plan) -> Self {
        let mut name = TextInput::new(&task.name);
        name.focused = true;
        let dur_str = if task.duration_days_target > 0.0 {
            format_days(task.duration_days_target)
        } else {
            String::new()
        };
        let (constraint_kind, constraint_val) = ConstraintSel::from_opt(task.constraint);
        let workers = task.workers.iter().map(WorkerSlotEdit::from_slot).collect();
        let task_id = &task.id;
        Self {
            mode: Mode::Edit(task.id),
            name,
            description: MultiLineInput::new(&task.description),
            duration: TextInput::new(&dur_str),
            focused: TextField::Name,
            status: plan.task_status(task_id),
            hovered_status: None,
            constraint_kind,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(constraint_val),
            actual_start: CalendarPicker::new(plan.task_actual_start(task_id)),
            actual_end: CalendarPicker::new(plan.task_actual_end(task_id)),
            open_calendar: None,
            workers,
            worker_scroll_y: 0.0,
            cursor_in_worker_list: false,
            open_slot_dropdown: None,
            slot_dropdown_hovered: None,
            slot_dropdown_scroll: 0,
            focused_slot_workload: None,
            hovered_plus: false,
            worker_error: false,
            dependencies: task
                .dependencies
                .iter()
                .map(|d| {
                    let lag_str = if d.lag_days != 0.0 {
                        format!("{}", d.lag_days)
                    } else {
                        String::new()
                    };
                    DependencyEdit {
                        target: Some(d.id),
                        dep_filter: TextInput::new(""),
                        lag_input: TextInput::new(lag_str),
                        hovered_target: false,
                        hovered_remove: false,
                    }
                })
                .collect(),
            dep_scroll_y: 0.0,
            cursor_in_dep_list: false,
            dep_dropdown_open_for: None,
            dep_dropdown_hovered: None,
            dep_dropdown_scroll: 0,
            focused_dep_lag: None,
            hovered_dep_plus: false,
            dep_error: false,
            name_error: false,
            duration_error: false,
            constraint_date_error: false,
            hovered_back: false,
            hovered_save: false,
            hovered_delete: false,
            actual_start_error: false,
            cursor_in_desc: false,
            actual_end_error: false,
            relaxed_mode: task.relaxed_mode,
            hovered_relaxed: false,
            form_scroll_y: 0.0,
            max_desc_scroll: Cell::new(0.0),
            scheduler_error: None,
        }
    }

    fn title(&self) -> &'static str {
        match self.mode {
            Mode::New => "Add Task",
            Mode::Edit(_) => "Edit Task",
        }
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    fn panel_rect(width: f32, height: f32) -> Rect {
        let pw = (width * 0.95).min(PANEL_W);
        let ph = (height * 0.95).min(PANEL_H);
        Rect::from_xywh((width - pw) / 2.0, (height - ph) / 2.0, pw, ph)
    }

    fn back_btn_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(
            p.left + BTN_INSET,
            p.top + BTN_INSET,
            BACK_BTN_SIZE,
            BACK_BTN_SIZE,
        )
    }

    fn effective_scroll(&self, width: f32, height: f32) -> f32 {
        let panel_h = Self::panel_rect(width, height).height();
        self.form_scroll_y.min((PANEL_H - panel_h).max(0.0))
    }

    fn save_btn_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(
            p.right - PLAN_FORM_PADDING - SAVE_BTN_W,
            p.top + PANEL_H - PLAN_FORM_PADDING - PLAN_BTN_H,
            SAVE_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn delete_btn_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        Rect::from_xywh(
            p.left + PLAN_FORM_PADDING,
            p.top + PANEL_H - PLAN_FORM_PADDING - PLAN_BTN_H,
            DELETE_BTN_W,
            PLAN_BTN_H,
        )
    }

    fn form_top(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn row_label_y(row: usize, width: f32, height: f32) -> f32 {
        let base = Self::form_top(width, height) + row as f32 * (FIELD_BLOCK_H + PLAN_FIELD_GAP);
        if row > ROW_DESC {
            base + (DESC_BLOCK_H - FIELD_BLOCK_H)
        } else {
            base
        }
    }

    fn full_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        let h = if row == ROW_DESC {
            DESC_H
        } else {
            PLAN_INPUT_H
        };
        Rect::from_xywh(x, y, w, h)
    }

    fn col_width(width: f32, height: f32) -> f32 {
        let inner = Self::panel_rect(width, height).width() - 2.0 * PLAN_FORM_PADDING;
        (inner - COL_GAP) / 2.0
    }

    fn left_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = Self::col_width(width, height);
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
    }

    fn right_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let cw = Self::col_width(width, height);
        let x = p.left + PLAN_FORM_PADDING + cw + COL_GAP;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, cw, PLAN_INPUT_H)
    }

    fn calendar_popup_rect(trigger: Rect, panel: Rect) -> Rect {
        let below = trigger.bottom + 4.0;
        let above = trigger.top - 4.0 - CAL_H;
        let top = if below + CAL_H <= panel.bottom + 8.0 {
            below
        } else {
            above
        };
        let left = (trigger.left + (trigger.width() - CAL_W) / 2.0)
            .max(panel.left + 4.0)
            .min(panel.right - CAL_W - 4.0);
        Rect::from_xywh(left, top, CAL_W, CAL_H)
    }

    // ── Worker layout ─────────────────────────────────────────────────────────

    fn workers_label_y(width: f32, height: f32) -> f32 {
        // After 6 field rows + their gaps, then one more gap before workers section
        // Also shift down by the extra height from the tall description row
        Self::form_top(width, height)
            + 6.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP)
            + (DESC_BLOCK_H - FIELD_BLOCK_H)
    }

    fn worker_list_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::workers_label_y(width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, WORKER_ROW_H * MAX_VISIBLE_WORKERS as f32)
    }

    fn worker_plus_rect(width: f32, height: f32) -> Rect {
        let list = Self::worker_list_rect(width, height);
        Rect::from_xywh(list.left, list.bottom, list.width(), PLUS_BTN_H)
    }

    fn slot_type_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * WORKER_ROW_H;
        let vy = row_y + (WORKER_ROW_H - WORKER_INPUT_H) / 2.0;
        Rect::from_xywh(list.left + WORKER_PAD_L, vy, SLOT_TYPE_W, WORKER_INPUT_H)
    }

    /// Rect of the user selector button for a visible slot (abs_idx = 0..workers.len()).
    fn slot_user_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * WORKER_ROW_H;
        let x = list.left + WORKER_PAD_L + SLOT_TYPE_W + WORKER_COL_GAP;
        let w = list.width()
            - WORKER_PAD_L
            - SLOT_TYPE_W
            - WORKER_COL_GAP
            - WORKER_COL_GAP
            - WORKER_WORKLOAD_W
            - WORKER_COL_GAP
            - WORKER_REMOVE_SIZE
            - WORKER_PAD_R;
        let vy = row_y + (WORKER_ROW_H - WORKER_INPUT_H) / 2.0;
        Rect::from_xywh(x, vy, w, WORKER_INPUT_H)
    }

    fn slot_workload_rect(list: Rect, abs_idx: usize) -> Rect {
        let user = Self::slot_user_rect(list, abs_idx);
        Rect::from_xywh(
            user.right + WORKER_COL_GAP,
            user.top,
            WORKER_WORKLOAD_W,
            user.height(),
        )
    }

    fn slot_remove_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * WORKER_ROW_H;
        Rect::from_xywh(
            list.right - WORKER_PAD_R - WORKER_REMOVE_SIZE,
            row_y + (WORKER_ROW_H - WORKER_REMOVE_SIZE) / 2.0,
            WORKER_REMOVE_SIZE,
            WORKER_REMOVE_SIZE,
        )
    }

    fn slot_dropdown_rect(list: Rect, abs_idx: usize, panel: Rect) -> Rect {
        let user_btn = Self::slot_user_rect(list, abs_idx);
        let below = user_btn.bottom + 2.0;
        let above = user_btn.top - 2.0 - USER_DROPDOWN_H;
        let top = if below + USER_DROPDOWN_H <= panel.bottom + 8.0 {
            below
        } else {
            above
        };
        Rect::from_xywh(user_btn.left, top, user_btn.width(), USER_DROPDOWN_H)
    }

    // ── Dependency layout ─────────────────────────────────────────────────────

    fn dep_label_y(width: f32, height: f32) -> f32 {
        let worker_plus = Self::worker_plus_rect(width, height);
        worker_plus.bottom + PLAN_FIELD_GAP
    }

    fn dep_list_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::dep_label_y(width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, DEP_ROW_H * MAX_VISIBLE_DEPS as f32)
    }

    fn dep_plus_rect(width: f32, height: f32) -> Rect {
        let list = Self::dep_list_rect(width, height);
        Rect::from_xywh(list.left, list.bottom, list.width(), PLUS_BTN_H)
    }

    fn dep_target_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * DEP_ROW_H;
        let vy = row_y + (DEP_ROW_H - DEP_INPUT_H) / 2.0;
        let w = list.width()
            - DEP_PAD_L
            - DEP_COL_GAP
            - DEP_LAG_W
            - DEP_COL_GAP
            - DEP_REMOVE_SIZE
            - DEP_PAD_R;
        Rect::from_xywh(list.left + DEP_PAD_L, vy, w, DEP_INPUT_H)
    }

    fn dep_lag_rect(list: Rect, abs_idx: usize) -> Rect {
        let target = Self::dep_target_rect(list, abs_idx);
        Rect::from_xywh(
            target.right + DEP_COL_GAP,
            target.top,
            DEP_LAG_W,
            DEP_INPUT_H,
        )
    }

    fn dep_remove_rect(list: Rect, abs_idx: usize) -> Rect {
        let row_y = list.top + abs_idx as f32 * DEP_ROW_H;
        Rect::from_xywh(
            list.right - DEP_PAD_R - DEP_REMOVE_SIZE,
            row_y + (DEP_ROW_H - DEP_REMOVE_SIZE) / 2.0,
            DEP_REMOVE_SIZE,
            DEP_REMOVE_SIZE,
        )
    }

    fn dep_dropdown_rect(list: Rect, abs_idx: usize, panel: Rect) -> Rect {
        let target = Self::dep_target_rect(list, abs_idx);
        let below = target.bottom + 2.0;
        let above = target.top - 2.0 - DEP_DROPDOWN_H;
        let top = if below + DEP_DROPDOWN_H <= panel.bottom + 8.0 {
            below
        } else {
            above
        };
        Rect::from_xywh(target.left, top, target.width(), DEP_DROPDOWN_H)
    }

    fn fwd_label_y(width: f32, height: f32) -> f32 {
        let dep_plus = Self::dep_plus_rect(width, height);
        dep_plus.bottom + PLAN_FIELD_GAP
    }

    fn fwd_list_rect(width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::fwd_label_y(width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, FWD_ROW_H * FWD_MAX_ROWS as f32)
    }

    // ── Segmented rects ───────────────────────────────────────────────────────

    fn status_btn_rects(width: f32, height: f32) -> [Rect; 5] {
        let r = Self::full_input_rect(ROW_STATUS, width, height);
        let bw = r.width() / 5.0;
        std::array::from_fn(|i| Rect::from_xywh(r.left + i as f32 * bw, r.top, bw, r.height()))
    }

    fn constraint_kind_btn_rects(width: f32, height: f32) -> [Rect; 4] {
        let r = Self::left_input_rect(ROW_CONSTRAINT, width, height);
        let bw = r.width() / 4.0;
        std::array::from_fn(|i| Rect::from_xywh(r.left + i as f32 * bw, r.top, bw, r.height()))
    }

    fn relaxed_btn_rect(width: f32, height: f32) -> Rect {
        Self::right_input_rect(ROW_DURATION, width, height)
    }

    // ── Calendar helpers ──────────────────────────────────────────────────────

    fn trigger_rect_for(target: OpenCalendar, width: f32, height: f32) -> Rect {
        match target {
            OpenCalendar::Constraint => Self::right_input_rect(ROW_CONSTRAINT, width, height),
            OpenCalendar::ActualStart => Self::left_input_rect(ROW_DATES, width, height),
            OpenCalendar::ActualEnd => Self::right_input_rect(ROW_DATES, width, height),
        }
    }

    fn picker_ref(&self, target: OpenCalendar) -> &CalendarPicker {
        match target {
            OpenCalendar::Constraint => &self.constraint_date,
            OpenCalendar::ActualStart => &self.actual_start,
            OpenCalendar::ActualEnd => &self.actual_end,
        }
    }

    fn picker_mut(&mut self, target: OpenCalendar) -> &mut CalendarPicker {
        match target {
            OpenCalendar::Constraint => &mut self.constraint_date,
            OpenCalendar::ActualStart => &mut self.actual_start,
            OpenCalendar::ActualEnd => &mut self.actual_end,
        }
    }

    fn cal_prev_year_btn(cal: Rect) -> Rect {
        Rect::from_xywh(
            cal.left + CAL_PAD,
            cal.top + CAL_PAD,
            CAL_HEADER_H,
            CAL_HEADER_H,
        )
    }

    fn cal_prev_month_btn(cal: Rect) -> Rect {
        Rect::from_xywh(
            cal.left + CAL_PAD + CAL_HEADER_H + 2.0,
            cal.top + CAL_PAD,
            CAL_HEADER_H,
            CAL_HEADER_H,
        )
    }

    fn cal_next_month_btn(cal: Rect) -> Rect {
        Rect::from_xywh(
            cal.right - CAL_PAD - 2.0 * CAL_HEADER_H - 2.0,
            cal.top + CAL_PAD,
            CAL_HEADER_H,
            CAL_HEADER_H,
        )
    }

    fn cal_next_year_btn(cal: Rect) -> Rect {
        Rect::from_xywh(
            cal.right - CAL_PAD - CAL_HEADER_H,
            cal.top + CAL_PAD,
            CAL_HEADER_H,
            CAL_HEADER_H,
        )
    }

    fn cal_clear_btn(cal: Rect) -> Rect {
        let w = 48.0;
        let h = 22.0;
        Rect::from_xywh(cal.right - CAL_PAD - w, cal.bottom - CAL_PAD - h, w, h)
    }

    fn cal_today_btn(cal: Rect) -> Rect {
        let w = 48.0;
        let h = 22.0;
        Rect::from_xywh(cal.left + CAL_PAD, cal.bottom - CAL_PAD - h, w, h)
    }

    fn cal_day_cell(cal: Rect, day_1_offset: u32, day: u32) -> Rect {
        let grid_idx = day_1_offset + day - 1;
        let col = grid_idx % 7;
        let row = grid_idx / 7;
        let grid_top = cal.top + CAL_PAD + CAL_HEADER_H + CAL_DOW_H;
        Rect::from_xywh(
            cal.left + CAL_PAD + col as f32 * CAL_CELL,
            grid_top + row as f32 * CAL_ROW_H,
            CAL_CELL,
            CAL_ROW_H,
        )
    }

    // ── Focus / open state ────────────────────────────────────────────────────

    fn set_focus(&mut self, field: TextField) {
        self.name.focused = field == TextField::Name;
        self.description.focused = field == TextField::Description;
        self.duration.focused = field == TextField::Duration;
        self.focused = field;
        self.focused_slot_workload = None;
        self.focused_dep_lag = None;
    }

    fn focused_input_mut(&mut self) -> &mut TextInput {
        match self.focused {
            TextField::Name => &mut self.name,
            TextField::Duration => &mut self.duration,
            TextField::None | TextField::Description => {
                unreachable!("description/none are not routed through focused_input_mut")
            }
        }
    }

    fn open_calendar_picker(&mut self, target: OpenCalendar) {
        if let Some(old) = self.open_calendar.take() {
            self.picker_mut(old).reset_hover();
        }
        self.open_calendar = Some(target);
        self.close_slot_dropdown();
        self.focused_slot_workload = None;
    }

    fn close_calendar(&mut self) {
        if let Some(old) = self.open_calendar.take() {
            self.picker_mut(old).reset_hover();
        }
    }

    fn open_slot_dropdown(&mut self, slot_idx: usize) {
        self.close_calendar();
        self.open_slot_dropdown = Some(slot_idx);
        self.slot_dropdown_hovered = None;
        self.slot_dropdown_scroll = 0;
        self.workers[slot_idx].user_filter = TextInput::new("");
        self.focused_slot_workload = None;
        // Unfocus text fields
        self.name.focused = false;
        self.description.focused = false;
        self.duration.focused = false;
    }

    fn close_slot_dropdown(&mut self) {
        if let Some(i) = self.open_slot_dropdown.take()
            && i < self.workers.len()
        {
            self.workers[i].user_filter = TextInput::new("");
        }
        self.slot_dropdown_hovered = None;
    }

    fn open_dep_dropdown(&mut self, dep_idx: usize) {
        self.close_calendar();
        self.close_slot_dropdown();
        self.dep_dropdown_open_for = Some(dep_idx);
        self.dep_dropdown_hovered = None;
        self.dep_dropdown_scroll = 0;
        self.dependencies[dep_idx].dep_filter = TextInput::new("");
        self.focused_dep_lag = None;
        self.name.focused = false;
        self.description.focused = false;
        self.duration.focused = false;
    }

    fn close_dep_dropdown(&mut self) {
        if let Some(i) = self.dep_dropdown_open_for.take()
            && i < self.dependencies.len()
        {
            self.dependencies[i].dep_filter = TextInput::new("");
        }
        self.dep_dropdown_hovered = None;
    }

    fn clamp_worker_scroll_y(&mut self) {
        let total_h = self.workers.len() as f32 * WORKER_ROW_H;
        let visible_h = WORKER_ROW_H * MAX_VISIBLE_WORKERS as f32;
        let max = (total_h - visible_h).max(0.0);
        self.worker_scroll_y = self.worker_scroll_y.clamp(0.0, max);
    }

    fn clamp_dep_scroll_y(&mut self) {
        let content_h = self.dependencies.len() as f32 * DEP_ROW_H;
        let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
        let max = (content_h - visible_h).max(0.0);
        self.dep_scroll_y = self.dep_scroll_y.clamp(0.0, max);
    }

    fn mode_task_id(&self) -> Option<TaskId> {
        match self.mode {
            Mode::Edit(id) => Some(id),
            Mode::New => None,
        }
    }

    // ── Submit ────────────────────────────────────────────────────────────────

    fn try_submit(&mut self, plan: &Plan, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        // Validate all fields and collect errors before returning so the user
        // can see every problem at once rather than one at a time.
        let name = self.name.content.trim().to_string();
        self.name_error = name.is_empty();

        let duration_str = self.duration.content.trim().to_string();
        let duration_parsed = duration_str.parse::<f32>().ok().filter(|&v| v > 0.0);
        self.duration_error = duration_parsed.is_none();

        let dependencies: Vec<Dependency> = self
            .dependencies
            .iter()
            .filter_map(|d| {
                let id = d.target?;
                let lag_days = d.lag_input.content.trim().parse::<f32>().unwrap_or(0.0);
                Some(Dependency { id, lag_days })
            })
            .collect();
        let dependencies = simplify_dependencies(dependencies, plan);
        self.dep_error = dependencies.is_empty();

        let worker_slots: Vec<WorkerSlot> = self
            .workers
            .iter()
            .filter_map(|s| s.to_worker_slot())
            .collect();
        self.worker_error = false;

        self.constraint_date_error =
            self.constraint_kind != ConstraintSel::None && self.constraint_date.value.is_none();

        self.actual_start_error =
            self.status != Status::NotStarted && self.actual_start.value.is_none();
        self.actual_end_error = matches!(self.status, Status::Complete | Status::Dropped)
            && self.actual_end.value.is_none();

        if self.name_error
            || self.duration_error
            || self.dep_error
            || self.constraint_date_error
            || self.actual_start_error
            || self.actual_end_error
        {
            self.scheduler_error = None;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        let duration = duration_parsed.unwrap();
        let description = self.description.content.trim().to_string();
        let constraint = self
            .constraint_kind
            .to_constraint(self.constraint_date.value);

        // Dry-run: clone the plan, apply the mutation, run the scheduler.
        // Only send the real request if the scheduler succeeds.
        let mut dry_plan = plan.clone();
        let sched_result: Result<(), String> = match self.mode {
            Mode::New => {
                let mut task = Task::new(name.clone(), description.clone());
                task.duration_days_target = duration;
                task.constraint = constraint;
                task.workers = worker_slots.clone();
                task.dependencies = dependencies.clone();
                let task_id = dry_plan.add_task(task);
                dry_plan.set_task_status(task_id, self.status);
                dry_plan.set_task_actual_start(task_id, self.actual_start.value);
                dry_plan.set_task_actual_end(task_id, self.actual_end.value);
                dry_plan
                    .compute_time_optimised_plan()
                    .map_err(|e| e.to_string())
            }
            Mode::Edit(id) => {
                let patch = TaskPatch::new()
                    .name(name.clone())
                    .description(description.clone())
                    .status(self.status)
                    .duration_days_target(duration)
                    .constraint(constraint)
                    .actual_start_date(self.actual_start.value)
                    .actual_end_date(self.actual_end.value)
                    .workers(worker_slots.clone())
                    .dependencies(dependencies.clone())
                    .relaxed_mode(self.relaxed_mode);
                apply_task_patch(&mut dry_plan, id, patch)
                    .map_err(|e| e.to_string())
                    .and_then(|()| {
                        dry_plan
                            .compute_time_optimised_plan()
                            .map_err(|e| e.to_string())
                    })
            }
        };

        if let Err(e) = sched_result {
            self.scheduler_error = Some(e);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        self.scheduler_error = None;

        match self.mode {
            Mode::New => {
                let mut task = Task::new(name, description);
                task.duration_days_target = duration;
                task.constraint = constraint;
                task.workers = worker_slots;
                task.dependencies = dependencies;
                task.relaxed_mode = self.relaxed_mode;
                let task_id = task.id;
                sender.send(PlanRequest::CreateTask(task));
                // Apply status/actual-dates immediately if non-default.
                if self.status != Status::NotStarted
                    || self.actual_start.value.is_some()
                    || self.actual_end.value.is_some()
                {
                    let patch = TaskPatch::new()
                        .status(self.status)
                        .actual_start_date(self.actual_start.value)
                        .actual_end_date(self.actual_end.value);
                    sender.send(PlanRequest::UpdateTask(task_id, patch));
                }
            }
            Mode::Edit(id) => {
                let patch = TaskPatch::new()
                    .name(name)
                    .description(description)
                    .status(self.status)
                    .duration_days_target(duration)
                    .constraint(constraint)
                    .actual_start_date(self.actual_start.value)
                    .actual_end_date(self.actual_end.value)
                    .workers(worker_slots)
                    .dependencies(dependencies)
                    .relaxed_mode(self.relaxed_mode);
                sender.send(PlanRequest::UpdateTask(id, patch));
            }
        }
        FloatingWindowOutcome::close()
    }
}
// }}}

// ── Drawing helpers ───────────────────────────────────────────────────────────

fn draw_multi_line_input(
    canvas: &Canvas,
    rect: Rect,
    input: &crate::ui::multi_line_input::MultiLineInput,
    focused: bool,
    cache: &RenderCache,
) {
    use crate::ui::multi_line_input::MultiLineInput;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if focused {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if focused { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let (_, metrics) = cache.font.metrics();
    let line_h = metrics.descent - metrics.ascent + 2.0;
    let text_x = rect.left + 8.0;
    let text_top = rect.top + 4.0;
    let inner_width = rect.width() - 16.0;
    let visible_h = rect.height() - 8.0;

    // Clamp scroll (never auto-scroll; that happens only in event handlers).
    input.clamp_scroll(inner_width, &cache.font, line_h, visible_h);
    let scroll_y = input.scroll_y.get();

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            rect.left + 1.0,
            rect.top + 1.0,
            rect.width() - 2.0,
            rect.height() - 2.0,
        ),
        ClipOp::Intersect,
        false,
    );

    let content = &input.content;
    let lines = input.visual_lines(inner_width, &cache.font);
    let link_ranges = MultiLineInput::find_links(content);

    if lines.is_empty() || (lines.len() == 1 && lines[0].text.is_empty() && content.is_empty()) {
        // Placeholder
        if let Some(blob) = TextBlob::new("Description…", &cache.font) {
            paint.set_color(Color::from(PLACEHOLDER_FG));
            canvas.draw_text_blob(&blob, (text_x, text_top - metrics.ascent), &paint);
        }
    } else {
        for (i, vline) in lines.iter().enumerate() {
            let y = text_top + i as f32 * line_h - scroll_y;
            if y + line_h < rect.top || y > rect.bottom() {
                continue;
            }
            if vline.text.is_empty() {
                continue;
            }

            // Render with link highlighting: split into spans
            let line_byte_start = vline.byte_start;
            let line_byte_end = line_byte_start + vline.text.len();

            // Collect link spans that overlap this line
            let mut spans: Vec<(usize, usize, bool)> = Vec::new(); // (start, end, is_link)
            let mut pos = line_byte_start;
            for range in &link_ranges {
                let link_start = range.start.max(line_byte_start).min(line_byte_end);
                let link_end = range.end.max(line_byte_start).min(line_byte_end);
                if link_start >= link_end {
                    continue;
                }
                if pos < link_start {
                    spans.push((pos, link_start, false));
                }
                spans.push((link_start, link_end, true));
                pos = link_end;
            }
            if pos < line_byte_end {
                spans.push((pos, line_byte_end, false));
            }

            if spans.is_empty() {
                // No links on this line
                if let Some(blob) = TextBlob::new(vline.text, &cache.font) {
                    paint.set_color(Color::from(INPUT_FG));
                    canvas.draw_text_blob(&blob, (text_x, y - metrics.ascent), &paint);
                }
            } else {
                let mut span_x = text_x;
                for (s_start, s_end, is_link) in spans {
                    let span_text = &content[s_start..s_end];
                    if span_text.is_empty() {
                        continue;
                    }
                    let span_w = cache.font.measure_str(span_text, None).0;
                    if let Some(blob) = TextBlob::new(span_text, &cache.font) {
                        paint.set_color(Color::from(if is_link { LINK_COLOR } else { INPUT_FG }));
                        canvas.draw_text_blob(&blob, (span_x, y - metrics.ascent), &paint);
                        if is_link {
                            // Underline
                            paint.set_style(PaintStyle::Stroke);
                            paint.set_stroke_width(1.0);
                            let underline_y = y - metrics.ascent + metrics.descent + 1.0;
                            canvas.draw_line(
                                (span_x, underline_y),
                                (span_x + span_w, underline_y),
                                &paint,
                            );
                            paint.set_style(PaintStyle::Fill);
                        }
                    }
                    span_x += span_w;
                }
            }
        }
    }

    // Cursor
    if focused {
        let cursor_pos = input.clamped_cursor();
        let (cursor_line_idx, cursor_col_str) = {
            let before = &content[..cursor_pos];
            let idx = lines
                .iter()
                .rposition(|l| l.byte_start <= cursor_pos)
                .unwrap_or(0);
            let col_start = lines.get(idx).map(|l| l.byte_start).unwrap_or(0);
            let col_str = before[col_start..].to_owned();
            (idx, col_str)
        };
        let cursor_x = text_x + cache.font.measure_str(&cursor_col_str, None).0;
        let cursor_y_top = text_top + cursor_line_idx as f32 * line_h - scroll_y;
        let cursor_y_bot = cursor_y_top + line_h;
        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_line((cursor_x, cursor_y_top), (cursor_x, cursor_y_bot), &paint);
        paint.set_style(PaintStyle::Fill);
    }

    // Scrollbar
    let total_h = lines.len() as f32 * line_h + 8.0;
    let scrollbar_visible_h = rect.height();
    let max_scroll = (total_h - scrollbar_visible_h).max(0.0);
    if max_scroll > 0.0 {
        let thumb_h = (scrollbar_visible_h * scrollbar_visible_h / total_h).max(20.0);
        let thumb_y = rect.top + (scroll_y / max_scroll) * (scrollbar_visible_h - thumb_h);
        paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(rect.right - 6.0, thumb_y, 4.0, thumb_h),
                2.0,
                2.0,
            ),
            &paint,
        );
    }

    canvas.restore();
}

fn draw_text_input(
    canvas: &Canvas,
    rect: Rect,
    input: &TextInput,
    focused: bool,
    error: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if error {
        Color::from(INPUT_BORDER_ERROR)
    } else if focused {
        Color::from(INPUT_BORDER_FOCUS)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if error { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let h_pad = 8.0;
    let inner = Rect::from_xywh(
        rect.left + h_pad,
        rect.top + 2.0,
        rect.width() - 2.0 * h_pad,
        rect.height() - 4.0,
    );

    // Compute cursor pixel position (needed for scroll even when not focused).
    let cursor_pos = input.cursor.min(input.content.len());
    let cursor_x_px = if cursor_pos == 0 {
        0.0f32
    } else {
        cache.font.measure_str(&input.content[..cursor_pos], None).0
    };

    // Keep cursor visible: update scroll_x so cursor stays inside inner rect.
    let scroll_x = if focused {
        let inner_w = inner.width();
        let prev = input.scroll_x.get();
        let next = if cursor_x_px < prev {
            cursor_x_px
        } else if cursor_x_px > prev + inner_w {
            cursor_x_px - inner_w + 8.0
        } else {
            prev
        };
        input.scroll_x.set(next);
        next
    } else {
        0.0
    };

    canvas.save();
    canvas.clip_rect(inner, ClipOp::Intersect, false);
    let (_, metrics) = cache.font.metrics();
    let text_y =
        rect.top + (rect.height() - (metrics.descent - metrics.ascent)) / 2.0 - metrics.ascent;
    if !input.content.is_empty()
        && let Some(blob) = TextBlob::new(&input.content, &cache.font)
    {
        paint.set_color(Color::from(INPUT_FG));
        canvas.draw_text_blob(&blob, (inner.left - scroll_x, text_y), &paint);
    }
    if focused {
        paint.set_color(Color::from(INPUT_CURSOR_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(
                inner.left + cursor_x_px - scroll_x,
                rect.top + 5.0,
                1.5,
                rect.height() - 10.0,
            ),
            &paint,
        );
    }
    canvas.restore();
}

fn draw_date_btn(
    canvas: &Canvas,
    rect: Rect,
    picker: &CalendarPicker,
    is_open: bool,
    disabled: bool,
    error: bool,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(if disabled {
        Color::from(SUBTLE_BG)
    } else if error {
        Color::from(ERROR_BG)
    } else {
        Color::from(INPUT_BG)
    });
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if disabled {
        Color::from(DIVIDER_COLOR)
    } else if error {
        Color::from(INPUT_BORDER_ERROR)
    } else if is_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else if picker.hovered_trigger {
        Color::from(MUTED_FG)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(if error { 2.0 } else { 1.0 });
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let text = picker.display_text();
    if let Some(blob) = TextBlob::new(&text, &cache.font) {
        let (_, m) = cache.font.metrics();
        let ty = rect.top + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(if disabled {
            Color::from(MUTED_FG)
        } else if picker.value.is_some() {
            Color::from(INPUT_FG)
        } else {
            Color::from(MUTED_FG)
        });
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Tiny calendar icon
    let icon_cx = rect.right - 16.0;
    let icon_cy = rect.top + rect.height() / 2.0;
    let hs = 5.0;
    paint.set_color(if disabled {
        Color::from(MUTED_FG)
    } else {
        Color::from(SUBTLE_FG)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.2);
    canvas.draw_rect(
        Rect::from_xywh(icon_cx - hs, icon_cy - hs * 0.8, hs * 2.0, hs * 1.8),
        &paint,
    );
    let mut pb = PathBuilder::new();
    pb.move_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx - hs + 1.5, icon_cy - hs * 0.8 - 3.0));
    pb.move_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8));
    pb.line_to((icon_cx + hs - 1.5, icon_cy - hs * 0.8 - 3.0));
    canvas.draw_path(&pb.detach(), &paint);
}

fn draw_segmented(
    canvas: &Canvas,
    rects: &[Rect],
    labels: &[&str],
    selected: usize,
    hovered: Option<usize>,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let n = rects.len();
    for (i, (rect, label)) in rects.iter().zip(labels.iter()).enumerate() {
        let is_first = i == 0;
        let is_last = i == n - 1;
        let is_sel = i == selected;
        let is_hov = hovered == Some(i);
        let bg = if is_sel {
            BTN_PRIMARY_BG
        } else if is_hov {
            TOOLBAR_BTN_HOVER_BG
        } else {
            BTN_SECONDARY_BG
        };

        canvas.save();
        canvas.clip_rect(*rect, ClipOp::Intersect, false);
        paint.set_color(Color::from(bg));
        paint.set_style(PaintStyle::Fill);
        let r = if is_first || is_last {
            PLAN_BTN_CORNER
        } else {
            0.0
        };
        canvas.draw_rrect(RRect::new_rect_xy(*rect, r, r), &paint);
        canvas.restore();

        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        if !is_last {
            canvas.draw_line((rect.right, rect.top), (rect.right, rect.bottom), &paint);
        }

        if let Some(blob) = TextBlob::new(label, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(label, None);
            let (_, sm) = cache.small_font.metrics();
            let tx = rect.left + (rect.width() - adv) / 2.0;
            let ty = rect.top + (rect.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
            paint.set_style(PaintStyle::Fill);
            paint.set_color(if is_sel {
                Color::from(BTN_PRIMARY_FG)
            } else {
                Color::from(BTN_SECONDARY_FG)
            });
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }
    }

    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
        let group = Rect::from_ltrb(first.left, first.top, last.right, last.bottom);
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(group, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
    }
}

fn draw_calendar_popup(
    canvas: &Canvas,
    cal: Rect,
    picker: &CalendarPicker,
    today: NaiveDate,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from(OVERLAY_LIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(cal.left + 2.0, cal.top + 4.0, cal.width(), cal.height()),
            CORNER,
            CORNER,
        ),
        &paint,
    );

    paint.set_color(Color::from(PANEL_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(RRect::new_rect_xy(cal, CORNER, CORNER), &paint);
    paint.set_style(PaintStyle::Fill);

    let (_, sm) = cache.small_font.metrics();
    let sm_h = sm.descent - sm.ascent;

    // 4 nav buttons: «prev_year  ‹prev_month  next_month›  next_year»
    let nav_btns = [
        (
            TaskFormWindow::cal_prev_year_btn(cal),
            picker.hovered_prev_year,
            -2i32,
        ),
        (
            TaskFormWindow::cal_prev_month_btn(cal),
            picker.hovered_prev_month,
            -1,
        ),
        (
            TaskFormWindow::cal_next_month_btn(cal),
            picker.hovered_next_month,
            1,
        ),
        (
            TaskFormWindow::cal_next_year_btn(cal),
            picker.hovered_next_year,
            2,
        ),
    ];

    for (btn, hov, dir) in nav_btns {
        let bg = if hov {
            TOOLBAR_BTN_HOVER_BG
        } else {
            INPUT_BG
        };
        paint.set_color(Color::from(bg));
        canvas.draw_rrect(
            RRect::new_rect_xy(btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        let cx = btn.left + btn.width() / 2.0;
        let cy = btn.top + btn.height() / 2.0;
        // Single chevron (month) or double chevron (year)
        let double = dir.abs() == 2;
        let s = 4.0;
        let offset = if double { 2.5 } else { 0.0 };
        paint.set_color(Color::from(ITEM_FG));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        let double_shifts = [-offset, offset];
        let single_shift = [0.0f32];
        let shifts: &[f32] = if double {
            &double_shifts
        } else {
            &single_shift
        };
        for shift in shifts {
            let ox = if dir < 0 { *shift } else { -*shift };
            let mut pb = PathBuilder::new();
            if dir < 0 {
                pb.move_to((cx + ox + s * 0.45, cy - s));
                pb.line_to((cx + ox - s * 0.45, cy));
                pb.line_to((cx + ox + s * 0.45, cy + s));
            } else {
                pb.move_to((cx + ox - s * 0.45, cy - s));
                pb.line_to((cx + ox + s * 0.45, cy));
                pb.line_to((cx + ox - s * 0.45, cy + s));
            }
            canvas.draw_path(&pb.detach(), &paint);
        }
        paint.set_style(PaintStyle::Fill);
    }

    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let month_str = format!(
        "{} {}",
        month_names[(picker.nav_month as usize).saturating_sub(1).min(11)],
        picker.nav_year
    );
    if let Some(blob) = TextBlob::new(&month_str, &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str(&month_str, None);
        let tx = cal.left + (cal.width() - adv) / 2.0;
        let ty =
            TaskFormWindow::cal_prev_year_btn(cal).top + (CAL_HEADER_H - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(ITEM_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    let dow_labels = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
    let dow_y_top = cal.top + CAL_PAD + CAL_HEADER_H;
    for (i, lbl) in dow_labels.iter().enumerate() {
        let cx = cal.left + CAL_PAD + i as f32 * CAL_CELL + CAL_CELL / 2.0;
        if let Some(blob) = TextBlob::new(lbl, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(lbl, None);
            let ty = dow_y_top + (CAL_DOW_H - sm_h) / 2.0 - sm.ascent;
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (cx - adv / 2.0, ty), &paint);
        }
    }

    let day_1_offset = first_weekday_offset(picker.nav_year, picker.nav_month);
    let num_days = days_in_month(picker.nav_year, picker.nav_month);
    let today_in_nav = today.year() == picker.nav_year && today.month() == picker.nav_month;

    for day in 1..=num_days {
        let cell = TaskFormWindow::cal_day_cell(cal, day_1_offset, day);
        let is_selected = picker
            .value
            .map(|v| v.year() == picker.nav_year && v.month() == picker.nav_month && v.day() == day)
            .unwrap_or(false);
        let is_today = today_in_nav && today.day() == day;
        let is_hov = picker.hovered_day == Some(day);
        let cx = cell.left + cell.width() / 2.0;
        let cy = cell.top + cell.height() / 2.0;

        if is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        } else if is_hov {
            paint.set_color(Color::from(CAL_SELECTED_BG));
            canvas.draw_circle((cx, cy), CAL_CELL / 2.0 - 2.0, &paint);
        }

        let day_str = format!("{}", day);
        if let Some(blob) = TextBlob::new(&day_str, &cache.small_font) {
            let (adv, _) = cache.small_font.measure_str(&day_str, None);
            let tx = cx - adv / 2.0;
            let ty = cell.top + (cell.height() - sm_h) / 2.0 - sm.ascent;
            paint.set_color(if is_selected {
                Color::WHITE
            } else if is_today {
                Color::from(BTN_PRIMARY_BG)
            } else {
                Color::from(ITEM_FG)
            });
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        if is_today && !is_selected {
            paint.set_color(Color::from(BTN_PRIMARY_BG));
            canvas.draw_circle((cx, cell.top + cell.height() - 4.0), 2.0, &paint);
        }
    }

    let footer_y = cal.bottom - CAL_PAD - CAL_FOOTER_H;
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(cal.left, footer_y, cal.width(), 1.0),
        &paint,
    );

    let clear_btn = TaskFormWindow::cal_clear_btn(cal);
    paint.set_color(Color::from(if picker.hovered_clear {
        TOOLBAR_BTN_HOVER_BG
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(clear_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Clear", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Clear", None);
        let tx = clear_btn.left + (clear_btn.width() - adv) / 2.0;
        let ty = clear_btn.top + (clear_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }

    let today_btn = TaskFormWindow::cal_today_btn(cal);
    paint.set_color(Color::from(if picker.hovered_today {
        TOOLBAR_BTN_HOVER_BG
    } else {
        BTN_SECONDARY_BG
    }));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(today_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    if let Some(blob) = TextBlob::new("Today", &cache.small_font) {
        let (adv, _) = cache.small_font.measure_str("Today", None);
        let tx = today_btn.left + (today_btn.width() - adv) / 2.0;
        let ty = today_btn.top + (today_btn.height() - sm_h) / 2.0 - sm.ascent;
        paint.set_color(Color::from(BTN_SECONDARY_FG));
        canvas.draw_text_blob(&blob, (tx, ty), &paint);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_worker_row(
    canvas: &Canvas,
    list: Rect,
    vis_idx: usize,
    slot: &WorkerSlotEdit,
    is_dropdown_open: bool,
    wl_focused: bool,
    plan: &Plan,
    cache: &RenderCache,
) {
    let type_rect = TaskFormWindow::slot_type_rect(list, vis_idx);
    let user_rect = TaskFormWindow::slot_user_rect(list, vis_idx);
    let wl_rect = TaskFormWindow::slot_workload_rect(list, vis_idx);
    let rm_rect = TaskFormWindow::slot_remove_rect(list, vis_idx);
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Row separator
    if vis_idx > 0 {
        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(
                list.left,
                list.top + vis_idx as f32 * WORKER_ROW_H,
                list.width(),
                1.0,
            ),
            &paint,
        );
    }

    // Type toggle: two small pills "T" | "P"
    {
        let half_w = type_rect.width() / 2.0;
        let labels = ["T", "P"];
        let sel_idx = match slot.slot_type {
            SlotType::Placeholder => 0usize,
            SlotType::Specific => 1,
        };
        for (i, lbl) in labels.iter().enumerate() {
            let rx = type_rect.left + i as f32 * half_w;
            let pill = Rect::from_xywh(rx, type_rect.top, half_w, type_rect.height());
            let selected = i == sel_idx;
            let hov = slot.hovered_type == Some(i);
            let bg = if selected {
                BTN_PRIMARY_BG
            } else if hov {
                TOOLBAR_BTN_HOVER_BG
            } else {
                SUBTLE_BG
            };
            paint.set_color(Color::from(bg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(pill, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            if let Some(blob) = TextBlob::new(lbl, &cache.small_font) {
                let (adv, _) = cache.small_font.measure_str(lbl, None);
                let (_, sm) = cache.small_font.metrics();
                let ty = pill.top + (pill.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
                paint.set_color(Color::from(if selected { BTN_PRIMARY_FG } else { ITEM_FG }));
                canvas.draw_text_blob(&blob, (pill.left + (half_w - adv) / 2.0, ty), &paint);
            }
        }
        // Border around whole toggle
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(type_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);
    }

    // User/tag selector button
    let rrect = RRect::new_rect_xy(user_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if is_dropdown_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else if slot.hovered_user_btn {
        Color::from(MUTED_FG)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    // Picker label text
    let picker_text: String = match slot.slot_type {
        SlotType::Specific => slot
            .user_id
            .and_then(|id| plan.user(&id))
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "Select person…".to_string()),
        SlotType::Placeholder => {
            if slot.required_tags.is_empty() {
                "Any user".to_string()
            } else {
                let mut names: Vec<&str> = plan
                    .tags
                    .iter()
                    .filter(|t| slot.required_tags.contains(&t.id))
                    .map(|t| t.name.as_str())
                    .collect();
                names.sort_unstable();
                names.join(", ")
            }
        }
    };
    let picker_color = match slot.slot_type {
        SlotType::Specific if slot.user_id.is_some() => Color::from(INPUT_FG),
        SlotType::Placeholder if !slot.required_tags.is_empty() => Color::from(INPUT_FG),
        _ => Color::from(MUTED_FG),
    };

    canvas.save();
    canvas.clip_rect(
        Rect::from_xywh(
            user_rect.left + 6.0,
            user_rect.top,
            user_rect.width() - 22.0,
            user_rect.height(),
        ),
        ClipOp::Intersect,
        false,
    );
    if let Some(blob) = TextBlob::new(&picker_text, &cache.small_font) {
        let (_, sm) = cache.small_font.metrics();
        let ty = user_rect.top + (user_rect.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
        paint.set_color(picker_color);
        canvas.draw_text_blob(&blob, (user_rect.left + 6.0, ty), &paint);
    }
    canvas.restore();

    // Chevron on picker button
    {
        let cx = user_rect.right - 12.0;
        let cy = user_rect.top + user_rect.height() / 2.0;
        let s = 3.5;
        let mut pb = PathBuilder::new();
        if is_dropdown_open {
            pb.move_to((cx - s, cy + s * 0.5));
            pb.line_to((cx, cy - s * 0.5));
            pb.line_to((cx + s, cy + s * 0.5));
        } else {
            pb.move_to((cx - s, cy - s * 0.5));
            pb.line_to((cx, cy + s * 0.5));
            pb.line_to((cx + s, cy - s * 0.5));
        }
        paint.set_color(Color::from(PLACEHOLDER_FG));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_path(&pb.detach(), &paint);
        paint.set_style(PaintStyle::Fill);
    }

    // Workload input
    draw_text_input(canvas, wl_rect, &slot.workload, wl_focused, false, cache);

    // "d" suffix drawn inside workload box (right-aligned)
    if let Some(blob) = TextBlob::new("d", &cache.small_font) {
        let (_, sm) = cache.small_font.metrics();
        let ty = wl_rect.top + (wl_rect.height() - (sm.descent - sm.ascent)) / 2.0 - sm.ascent;
        paint.set_color(Color::from(LABEL_FG));
        canvas.draw_text_blob(&blob, (wl_rect.right - 14.0, ty), &paint);
    }

    // Remove button (tags-style: circle hover, ICON_DELETE_COLOR on hover)
    if slot.hovered_remove {
        let r = rm_rect.width().min(rm_rect.height()) / 2.0 - 2.0;
        let cx = rm_rect.left + rm_rect.width() / 2.0;
        let cy = rm_rect.top + rm_rect.height() / 2.0;
        paint.set_color(Color::from(ERROR_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_circle((cx, cy), r, &paint);
        paint.set_style(PaintStyle::Fill);
    }
    {
        let cx = rm_rect.left + rm_rect.width() / 2.0;
        let cy = rm_rect.top + rm_rect.height() / 2.0;
        let s = 5.0;
        let mut pb = PathBuilder::new();
        pb.move_to((cx - s, cy - s));
        pb.line_to((cx + s, cy + s));
        pb.move_to((cx + s, cy - s));
        pb.line_to((cx - s, cy + s));
        paint.set_color(if slot.hovered_remove {
            Color::from(ICON_DELETE_COLOR)
        } else {
            Color::from(OVERLAY_DARK)
        });
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.5);
        canvas.draw_path(&pb.detach(), &paint);
        paint.set_style(PaintStyle::Fill);
    }
}

fn draw_user_dropdown(
    canvas: &Canvas,
    dd: Rect,
    slot: &WorkerSlotEdit,
    hovered_row: Option<usize>,
    scroll: usize,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    // Filter input
    let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), USER_DROPDOWN_FILTER_H);
    draw_text_input(canvas, filter_rect, &slot.user_filter, true, false, cache);

    // Divider
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + USER_DROPDOWN_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    // User list
    let filtered = slot.filtered_users(plan);
    let list_top = dd.top + USER_DROPDOWN_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        dd.height() - USER_DROPDOWN_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if filtered.is_empty() {
        let msg = if slot.user_filter.content.trim().is_empty() {
            "No users in plan"
        } else {
            "No matches"
        };
        if let Some(blob) = TextBlob::new(msg, &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (dd.left + 8.0, list_top + 8.0 - sm.ascent), &paint);
        }
    } else {
        let end = (scroll + MAX_USER_DROPDOWN_ROWS).min(filtered.len());
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        for (vis, (uid, user)) in filtered[scroll..end].iter().enumerate() {
            let abs = scroll + vis;
            let ry = list_top + vis as f32 * USER_DROPDOWN_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), USER_DROPDOWN_ROW_H);

            if hovered_row == Some(abs) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            // Tick if already selected
            if slot.user_id == Some(**uid) {
                let tx = dd.left + 10.0;
                let ty = ry + USER_DROPDOWN_ROW_H / 2.0;
                paint.set_color(Color::from(BTN_PRIMARY_BG));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.5);
                let mut pb = PathBuilder::new();
                pb.move_to((tx, ty));
                pb.line_to((tx + 3.0, ty + 3.0));
                pb.line_to((tx + 7.0, ty - 3.0));
                canvas.draw_path(&pb.detach(), &paint);
                paint.set_style(PaintStyle::Fill);
            }

            if let Some(blob) = TextBlob::new(&user.name, &cache.small_font) {
                let ty = ry + (USER_DROPDOWN_ROW_H - sm_h) / 2.0 - sm.ascent;
                paint.set_color(Color::from(ITEM_FG));
                canvas.draw_text_blob(&blob, (dd.left + 22.0, ty), &paint);
            }
        }
    }

    canvas.restore();
}

fn draw_tag_dropdown(
    canvas: &Canvas,
    dd: Rect,
    slot: &WorkerSlotEdit,
    hovered_row: Option<usize>,
    scroll: usize,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    // Filter input
    let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), USER_DROPDOWN_FILTER_H);
    draw_text_input(canvas, filter_rect, &slot.tag_filter, true, false, cache);

    // Divider
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + USER_DROPDOWN_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    // Tag list
    let filtered = slot.filtered_tags(plan);
    let list_top = dd.top + USER_DROPDOWN_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        dd.height() - USER_DROPDOWN_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if filtered.is_empty() {
        let msg = if slot.tag_filter.content.trim().is_empty() {
            "No tags in plan"
        } else {
            "No matches"
        };
        if let Some(blob) = TextBlob::new(msg, &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (dd.left + 8.0, list_top + 8.0 - sm.ascent), &paint);
        }
    } else {
        let end = (scroll + MAX_USER_DROPDOWN_ROWS).min(filtered.len());
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        for (vis, tag) in filtered[scroll..end].iter().enumerate() {
            let abs = scroll + vis;
            let ry = list_top + vis as f32 * USER_DROPDOWN_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), USER_DROPDOWN_ROW_H);

            if hovered_row == Some(abs) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            // Tick if selected
            let selected = slot.required_tags.contains(&tag.id);
            if selected {
                let tx = dd.left + 10.0;
                let ty = ry + USER_DROPDOWN_ROW_H / 2.0;
                paint.set_color(Color::from(BTN_PRIMARY_BG));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.5);
                let mut pb = PathBuilder::new();
                pb.move_to((tx, ty));
                pb.line_to((tx + 3.0, ty + 3.0));
                pb.line_to((tx + 7.0, ty - 3.0));
                canvas.draw_path(&pb.detach(), &paint);
                paint.set_style(PaintStyle::Fill);
            }

            if let Some(blob) = TextBlob::new(&tag.name, &cache.small_font) {
                let ty = ry + (USER_DROPDOWN_ROW_H - sm_h) / 2.0 - sm.ascent;
                paint.set_color(Color::from(ITEM_FG));
                canvas.draw_text_blob(&blob, (dd.left + 22.0, ty), &paint);
            }
        }
    }

    canvas.restore();
}

#[allow(clippy::too_many_arguments)]
fn draw_dep_dropdown(
    canvas: &Canvas,
    dd: Rect,
    dep: &DependencyEdit,
    hovered_row: Option<usize>,
    scroll: usize,
    edit_task_id: Option<TaskId>,
    plan: &Plan,
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    // Shadow
    paint.set_color(Color::from(OVERLAY_XLIGHT));
    canvas.draw_rrect(
        RRect::new_rect_xy(
            Rect::from_xywh(dd.left + 2.0, dd.top + 3.0, dd.width(), dd.height()),
            PLAN_BTN_CORNER,
            PLAN_BTN_CORNER,
        ),
        &paint,
    );

    // Background
    paint.set_color(Color::from(INPUT_BG));
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_color(Color::from(INPUT_BORDER_FOCUS));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(
        RRect::new_rect_xy(dd, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    paint.set_style(PaintStyle::Fill);

    // Filter input
    let filter_rect = Rect::from_xywh(dd.left, dd.top, dd.width(), DEP_DROPDOWN_FILTER_H);
    draw_text_input(canvas, filter_rect, &dep.dep_filter, true, false, cache);

    // Divider
    paint.set_color(Color::from(DIVIDER_COLOR));
    canvas.draw_rect(
        Rect::from_xywh(dd.left, dd.top + DEP_DROPDOWN_FILTER_H, dd.width(), 1.0),
        &paint,
    );

    let filter = dep.dep_filter.content.to_lowercase();
    let mut items: Vec<(NodeId, String)> = Vec::new();

    // Plan Start
    if filter.is_empty() || "plan start".contains(filter.as_str()) {
        items.push((NodeId::PlanStart, "Plan Start".to_string()));
    }

    // Tasks
    let mut task_items: Vec<(NodeId, String)> = plan
        .tasks
        .iter()
        .filter(|(id, t)| {
            (edit_task_id != Some(**id))
                && (filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()))
        })
        .map(|(id, t)| (NodeId::Task(*id), t.name.clone()))
        .collect();
    task_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(task_items);

    // Milestones
    let mut ms_items: Vec<(NodeId, String)> = plan
        .milestones
        .iter()
        .filter(|(_, m)| filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()))
        .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone()))
        .collect();
    ms_items.sort_by(|a, b| a.1.cmp(&b.1));
    items.extend(ms_items);

    let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
    let list_rect = Rect::from_xywh(
        dd.left,
        list_top,
        dd.width(),
        dd.height() - DEP_DROPDOWN_FILTER_H - 1.0,
    );

    canvas.save();
    canvas.clip_rect(list_rect, ClipOp::Intersect, false);

    if items.is_empty() {
        let msg = "No matches";
        if let Some(blob) = TextBlob::new(msg, &cache.small_font) {
            let (_, sm) = cache.small_font.metrics();
            paint.set_color(Color::from(MUTED_FG));
            canvas.draw_text_blob(&blob, (dd.left + 8.0, list_top + 8.0 - sm.ascent), &paint);
        }
    } else {
        let end = (scroll + MAX_DEP_DROPDOWN_ROWS).min(items.len());
        let (_, sm) = cache.small_font.metrics();
        let sm_h = sm.descent - sm.ascent;
        for (vis, (node_id, name)) in items[scroll..end].iter().enumerate() {
            let abs = scroll + vis;
            let ry = list_top + vis as f32 * DEP_DROPDOWN_ROW_H;
            let row_rect = Rect::from_xywh(dd.left, ry, dd.width(), DEP_DROPDOWN_ROW_H);

            if hovered_row == Some(abs) {
                paint.set_color(Color::from(LIST_ITEM_HOVER_BG));
                canvas.draw_rect(row_rect, &paint);
            }

            // Tick if already selected
            if dep.target == Some(*node_id) {
                let tx = dd.left + 10.0;
                let ty = ry + DEP_DROPDOWN_ROW_H / 2.0;
                paint.set_color(Color::from(BTN_PRIMARY_BG));
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.5);
                let mut pb = PathBuilder::new();
                pb.move_to((tx, ty));
                pb.line_to((tx + 3.0, ty + 3.0));
                pb.line_to((tx + 7.0, ty - 3.0));
                canvas.draw_path(&pb.detach(), &paint);
                paint.set_style(PaintStyle::Fill);
            }

            if let Some(blob) = TextBlob::new(name, &cache.small_font) {
                let ty = ry + (DEP_DROPDOWN_ROW_H - sm_h) / 2.0 - sm.ascent;
                let fg = if *node_id == NodeId::PlanStart {
                    DEP_PLAN_START_FG
                } else {
                    ITEM_FG
                };
                paint.set_color(Color::from(fg));
                canvas.draw_text_blob(&blob, (dd.left + 22.0, ty), &paint);
            }
        }
    }

    canvas.restore();
}

// ── FloatingWindow impl ───────────────────────────────────────────────────────

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl FloatingWindow for TaskFormWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let save_btn = Self::save_btn_rect(width, height);
        let delete_btn = Self::delete_btn_rect(width, height);
        let today = chrono::Local::now().date_naive();
        let scroll_y = self.effective_scroll(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from(OVERLAY_SOFT));
        canvas.draw_rrect(
            RRect::new_rect_xy(
                Rect::from_xywh(
                    panel.left + 2.0,
                    panel.top + 4.0,
                    panel.width(),
                    panel.height(),
                ),
                CORNER,
                CORNER,
            ),
            &paint,
        );

        // Panel background
        paint.set_color(Color::from(PANEL_BG));
        canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);

        // Title bar
        let title_rect = Rect::from_xywh(panel.left, panel.top, panel.width(), TITLE_H);
        paint.set_color(Color::from(LIST_BG));
        canvas.draw_rrect(RRect::new_rect_xy(title_rect, CORNER, CORNER), &paint);
        canvas.draw_rect(
            Rect::from_xywh(
                panel.left,
                panel.top + CORNER,
                panel.width(),
                TITLE_H - CORNER,
            ),
            &paint,
        );

        let title = self.title();
        if let Some(blob) = TextBlob::new(title, &cache.font) {
            let (_, m) = cache.font.metrics();
            let (adv, _) = cache.font.measure_str(title, None);
            let tx = panel.left + (panel.width() - adv) / 2.0;
            let ty = panel.top + (TITLE_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(ITEM_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        crate::ui::window_chrome::draw_chevron_btn(canvas, back_btn, self.hovered_back);

        paint.set_color(Color::from(DIVIDER_COLOR));
        canvas.draw_rect(
            Rect::from_xywh(panel.left, panel.top + TITLE_H, panel.width(), 1.0),
            &paint,
        );

        // Clip to content zone (below title bar) and apply vertical scroll
        let content_clip = Rect::from_xywh(
            panel.left,
            panel.top + TITLE_H + 1.0,
            panel.width(),
            panel.height() - TITLE_H - 1.0,
        );
        canvas.save();
        canvas.clip_rect(content_clip, ClipOp::Intersect, false);
        canvas.translate((0.0, -scroll_y));

        let lx = panel.left + PLAN_FORM_PADDING;
        let (_, sm) = cache.small_font.metrics();
        let lyo = -sm.ascent;

        macro_rules! label {
            ($row:expr, $text:expr) => {
                if let Some(blob) = TextBlob::new($text, &cache.small_font) {
                    paint.set_color(Color::from(LABEL_FG));
                    canvas.draw_text_blob(
                        &blob,
                        (lx, Self::row_label_y($row, width, height) + lyo),
                        &paint,
                    );
                }
            };
            ($row:expr, $col:expr, $text:expr) => {
                if let Some(blob) = TextBlob::new($text, &cache.small_font) {
                    paint.set_color(Color::from(LABEL_FG));
                    let rx = match $col {
                        0 => lx,
                        _ => lx + Self::col_width(width, height) + COL_GAP,
                    };
                    canvas.draw_text_blob(
                        &blob,
                        (rx, Self::row_label_y($row, width, height) + lyo),
                        &paint,
                    );
                }
            };
        }

        // Name
        label!(ROW_NAME, "Name");
        draw_text_input(
            canvas,
            Self::full_input_rect(ROW_NAME, width, height),
            &self.name,
            self.focused == TextField::Name,
            self.name_error,
            cache,
        );

        // Description
        label!(ROW_DESC, "Description");
        draw_multi_line_input(
            canvas,
            Self::full_input_rect(ROW_DESC, width, height),
            &self.description,
            self.focused == TextField::Description,
            cache,
        );
        // Cache the max scroll for the description box so on_scroll can use it
        // without needing access to the font.
        {
            let desc_rect = Self::full_input_rect(ROW_DESC, width, height);
            let inner_w = desc_rect.width() - 16.0;
            let line_count = self
                .description
                .visual_lines(inner_w, &cache.font)
                .len()
                .max(1);
            let total_h = line_count as f32 * DESC_LINE_H + 8.0;
            self.max_desc_scroll.set((total_h - DESC_H).max(0.0));
        }

        // Status
        label!(ROW_STATUS, "Status");
        let status_rects = Self::status_btn_rects(width, height);
        let status_labels = [
            "Not Started",
            "In Progress",
            "On Hold",
            "Complete",
            "Dropped",
        ];
        let status_sel = match self.status {
            Status::NotStarted => 0,
            Status::InProgress => 1,
            Status::OnHold => 2,
            Status::Complete => 3,
            Status::Dropped => 4,
        };
        draw_segmented(
            canvas,
            &status_rects,
            &status_labels,
            status_sel,
            self.hovered_status,
            cache,
        );

        // Duration
        label!(ROW_DURATION, 0, "Duration (days)");
        draw_text_input(
            canvas,
            Self::left_input_rect(ROW_DURATION, width, height),
            &self.duration,
            self.focused == TextField::Duration,
            self.duration_error,
            cache,
        );
        // Relaxed mode toggle (default = strict / relaxed_mode false)
        label!(ROW_DURATION, 1, "Mode");
        {
            let btn_rect = Self::relaxed_btn_rect(width, height);
            let (bg, label_text) = if self.relaxed_mode {
                (
                    if self.hovered_relaxed {
                        TOOLBAR_BTN_HOVER_BG
                    } else {
                        SUBTLE_BG
                    },
                    "Relaxed",
                )
            } else {
                (BTN_PRIMARY_BG, "Strict")
            };
            paint.set_color(Color::from(bg));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(btn_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            paint.set_color(Color::from(INPUT_BORDER));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.0);
            canvas.draw_rrect(
                RRect::new_rect_xy(btn_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            paint.set_style(PaintStyle::Fill);
            if let Some(blob) = TextBlob::new(label_text, &cache.small_font) {
                let (_, sm2) = cache.small_font.metrics();
                let (adv, _) = cache.small_font.measure_str(label_text, None);
                let tx = btn_rect.left + (btn_rect.width() - adv) / 2.0;
                let ty = btn_rect.top + (btn_rect.height() - (sm2.descent - sm2.ascent)) / 2.0
                    - sm2.ascent;
                paint.set_color(Color::from(if self.relaxed_mode {
                    BTN_SECONDARY_FG
                } else {
                    BTN_PRIMARY_FG
                }));
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
        }

        // Constraint
        label!(ROW_CONSTRAINT, 0, "Constraint Type");
        let ck_rects = Self::constraint_kind_btn_rects(width, height);
        let ck_labels = ["None", "Earliest", "Fixed", "Latest"];
        let ck_sel = match self.constraint_kind {
            ConstraintSel::None => 0,
            ConstraintSel::Earliest => 1,
            ConstraintSel::Fixed => 2,
            ConstraintSel::Latest => 3,
        };
        draw_segmented(
            canvas,
            &ck_rects,
            &ck_labels,
            ck_sel,
            self.hovered_constraint_kind,
            cache,
        );

        label!(ROW_CONSTRAINT, 1, "Constraint Date");
        let constraint_trigger = Self::right_input_rect(ROW_CONSTRAINT, width, height);
        let constraint_open = self.open_calendar == Some(OpenCalendar::Constraint);
        if self.constraint_kind == ConstraintSel::None {
            let mut p2 = Paint::default();
            p2.set_anti_alias(true);
            p2.set_color(Color::from(SUBTLE_BG));
            p2.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(constraint_trigger, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &p2,
            );
            p2.set_color(Color::from(DIVIDER_COLOR));
            p2.set_style(PaintStyle::Stroke);
            p2.set_stroke_width(1.0);
            canvas.draw_rrect(
                RRect::new_rect_xy(constraint_trigger, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &p2,
            );
        } else {
            draw_date_btn(
                canvas,
                constraint_trigger,
                &self.constraint_date,
                constraint_open,
                false,
                self.constraint_date_error,
                cache,
            );
        }

        // Actual dates
        let start_disabled = self.status == Status::NotStarted;
        let end_disabled = !matches!(self.status, Status::Complete | Status::Dropped);
        label!(ROW_DATES, 0, "Actual Start");
        draw_date_btn(
            canvas,
            Self::left_input_rect(ROW_DATES, width, height),
            &self.actual_start,
            self.open_calendar == Some(OpenCalendar::ActualStart),
            start_disabled,
            self.actual_start_error,
            cache,
        );
        label!(ROW_DATES, 1, "Actual End");
        draw_date_btn(
            canvas,
            Self::right_input_rect(ROW_DATES, width, height),
            &self.actual_end,
            self.open_calendar == Some(OpenCalendar::ActualEnd),
            end_disabled,
            self.actual_end_error,
            cache,
        );

        // Workers section
        let wl_y = Self::workers_label_y(width, height);
        if let Some(blob) = TextBlob::new("Workers", &cache.small_font) {
            paint.set_color(Color::from(LABEL_FG));
            canvas.draw_text_blob(&blob, (lx, wl_y + lyo), &paint);
        }

        let list = Self::worker_list_rect(width, height);

        // List border
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(list, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);

        // Clip list to its rect
        canvas.save();
        canvas.clip_rect(list, ClipOp::Intersect, false);
        canvas.translate((0.0, -self.worker_scroll_y));

        if self.workers.is_empty() {
            // Empty state
            if let Some(blob) = TextBlob::new("No workers added yet", &cache.small_font) {
                let (_, sm2) = cache.small_font.metrics();
                let ty = list.top + (WORKER_ROW_H - (sm2.descent - sm2.ascent)) / 2.0 - sm2.ascent;
                paint.set_color(Color::from(MUTED_FG));
                canvas.draw_text_blob(&blob, (list.left + 12.0, ty), &paint);
            }
        } else {
            for abs in 0..self.workers.len() {
                let wl_focused = self.focused_slot_workload == Some(abs);
                let dd_open = self.open_slot_dropdown == Some(abs);
                draw_worker_row(
                    canvas,
                    list,
                    abs,
                    &self.workers[abs],
                    dd_open,
                    wl_focused,
                    plan,
                    cache,
                );
            }
        }

        canvas.restore();

        // Worker list scrollbar
        let total_worker_h = self.workers.len() as f32 * WORKER_ROW_H;
        let visible_worker_h = list.height();
        let max_wscroll = (total_worker_h - visible_worker_h).max(0.0);
        if max_wscroll > 0.0 {
            let thumb_h = (visible_worker_h * visible_worker_h / total_worker_h).max(20.0);
            let thumb_y =
                list.top + (self.worker_scroll_y / max_wscroll) * (visible_worker_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        list.right - SCROLLBAR_W - 2.0,
                        thumb_y,
                        SCROLLBAR_W,
                        thumb_h,
                    ),
                    2.0,
                    2.0,
                ),
                &paint,
            );
        }

        // Plus button
        let plus_rect = Self::worker_plus_rect(width, height);
        paint.set_color(Color::from(if self.hovered_plus {
            TOOLBAR_BTN_HOVER_BG
        } else {
            SUBTLE_BG
        }));
        canvas.draw_rrect(
            RRect::new_rect_xy(plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);
        {
            let cx = plus_rect.left + plus_rect.width() / 2.0;
            let cy = plus_rect.top + plus_rect.height() / 2.0;
            let s = 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to((cx - s, cy));
            pb.line_to((cx + s, cy));
            pb.move_to((cx, cy - s));
            pb.line_to((cx, cy + s));
            paint.set_color(Color::from(TOOLBAR_BTN_ICON_COLOR));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
        }

        // Dependencies section
        let dep_lbl_y = Self::dep_label_y(width, height);
        let dep_label_text = if self.dep_error {
            "Dependencies (at least one required)"
        } else {
            "Dependencies"
        };
        if let Some(blob) = TextBlob::new(dep_label_text, &cache.small_font) {
            paint.set_color(Color::from(if self.dep_error {
                BTN_DANGER_BG
            } else {
                LABEL_FG
            }));
            canvas.draw_text_blob(&blob, (lx, dep_lbl_y + lyo), &paint);
        }

        let dep_list = Self::dep_list_rect(width, height);

        paint.set_color(Color::from(if self.dep_error {
            BTN_DANGER_BG
        } else {
            INPUT_BORDER
        }));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(if self.dep_error { 2.0 } else { 1.0 });
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_list, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);

        canvas.save();
        canvas.clip_rect(dep_list, ClipOp::Intersect, false);
        canvas.translate((0.0, -self.dep_scroll_y));

        if self.dependencies.is_empty() {
            if let Some(blob) = TextBlob::new("No dependencies added yet", &cache.small_font) {
                let (_, sm2) = cache.small_font.metrics();
                let ty = dep_list.top + (DEP_ROW_H - (sm2.descent - sm2.ascent)) / 2.0 - sm2.ascent;
                paint.set_color(Color::from(MUTED_FG));
                canvas.draw_text_blob(&blob, (dep_list.left + 12.0, ty), &paint);
            }
        } else {
            for (abs, dep) in self.dependencies.iter().enumerate() {
                // Row separator
                if abs > 0 {
                    paint.set_color(Color::from(DIVIDER_COLOR));
                    canvas.draw_rect(
                        Rect::from_xywh(
                            dep_list.left,
                            dep_list.top + abs as f32 * DEP_ROW_H,
                            dep_list.width(),
                            1.0,
                        ),
                        &paint,
                    );
                }

                let target_rect = Self::dep_target_rect(dep_list, abs);
                let lag_rect = Self::dep_lag_rect(dep_list, abs);
                let rm_rect = Self::dep_remove_rect(dep_list, abs);
                let dd_open = self.dep_dropdown_open_for == Some(abs);

                // Target button
                let rrect = RRect::new_rect_xy(target_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
                paint.set_color(Color::from(INPUT_BG));
                paint.set_style(PaintStyle::Fill);
                canvas.draw_rrect(rrect, &paint);
                paint.set_color(if dd_open {
                    Color::from(INPUT_BORDER_FOCUS)
                } else if dep.hovered_target {
                    Color::from(MUTED_FG)
                } else {
                    Color::from(INPUT_BORDER)
                });
                paint.set_style(PaintStyle::Stroke);
                paint.set_stroke_width(1.0);
                canvas.draw_rrect(rrect, &paint);
                paint.set_style(PaintStyle::Fill);

                let target_name: String = match dep.target {
                    Some(NodeId::PlanStart) => "Plan Start".to_string(),
                    Some(NodeId::Task(id)) => plan
                        .tasks
                        .get(&id)
                        .map(|t| t.name.clone())
                        .unwrap_or_default(),
                    Some(NodeId::Milestone(id)) => plan
                        .milestones
                        .get(&id)
                        .map(|m| m.name.clone())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                let (target_text, target_color) = if dep.target.is_none() {
                    ("Select dependency…".to_string(), Color::from(MUTED_FG))
                } else if dep.target == Some(NodeId::PlanStart) {
                    (target_name, Color::from(DEP_PLAN_START_FG))
                } else {
                    (target_name, Color::from(INPUT_FG))
                };

                canvas.save();
                canvas.clip_rect(
                    Rect::from_xywh(
                        target_rect.left + 6.0,
                        target_rect.top,
                        target_rect.width() - 22.0,
                        target_rect.height(),
                    ),
                    ClipOp::Intersect,
                    false,
                );
                if let Some(blob) = TextBlob::new(&target_text, &cache.small_font) {
                    let (_, sm) = cache.small_font.metrics();
                    let ty = target_rect.top
                        + (target_rect.height() - (sm.descent - sm.ascent)) / 2.0
                        - sm.ascent;
                    paint.set_color(target_color);
                    canvas.draw_text_blob(&blob, (target_rect.left + 6.0, ty), &paint);
                }
                canvas.restore();

                // Chevron on target button
                {
                    let cx = target_rect.right - 12.0;
                    let cy = target_rect.top + target_rect.height() / 2.0;
                    let s = 3.5;
                    let mut pb = PathBuilder::new();
                    if dd_open {
                        pb.move_to((cx - s, cy + s * 0.5));
                        pb.line_to((cx, cy - s * 0.5));
                        pb.line_to((cx + s, cy + s * 0.5));
                    } else {
                        pb.move_to((cx - s, cy - s * 0.5));
                        pb.line_to((cx, cy + s * 0.5));
                        pb.line_to((cx + s, cy - s * 0.5));
                    }
                    paint.set_color(Color::from(PLACEHOLDER_FG));
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&pb.detach(), &paint);
                    paint.set_style(PaintStyle::Fill);
                }

                // Lag input
                let lag_focused = self.focused_dep_lag == Some(abs);
                draw_text_input(canvas, lag_rect, &dep.lag_input, lag_focused, false, cache);
                if dep.lag_input.content.is_empty()
                    && !lag_focused
                    && let Some(blob) = TextBlob::new("0", &cache.small_font)
                {
                    let (_, sm) = cache.small_font.metrics();
                    let ty = lag_rect.top + (lag_rect.height() - (sm.descent - sm.ascent)) / 2.0
                        - sm.ascent;
                    paint.set_color(Color::from(GHOST_FG));
                    canvas.draw_text_blob(&blob, (lag_rect.left + 8.0, ty), &paint);
                }

                // Remove button (tags-style: circle hover, ICON_DELETE_COLOR on hover)
                if dep.hovered_remove {
                    let r = rm_rect.width().min(rm_rect.height()) / 2.0 - 2.0;
                    let cx = rm_rect.left + rm_rect.width() / 2.0;
                    let cy = rm_rect.top + rm_rect.height() / 2.0;
                    paint.set_color(Color::from(ERROR_BG));
                    paint.set_style(PaintStyle::Fill);
                    canvas.draw_circle((cx, cy), r, &paint);
                    paint.set_style(PaintStyle::Fill);
                }
                {
                    let cx = rm_rect.left + rm_rect.width() / 2.0;
                    let cy = rm_rect.top + rm_rect.height() / 2.0;
                    let s = 5.0;
                    let mut pb = PathBuilder::new();
                    pb.move_to((cx - s, cy - s));
                    pb.line_to((cx + s, cy + s));
                    pb.move_to((cx + s, cy - s));
                    pb.line_to((cx - s, cy + s));
                    paint.set_color(if dep.hovered_remove {
                        Color::from(ICON_DELETE_COLOR)
                    } else {
                        Color::from(OVERLAY_DARK)
                    });
                    paint.set_style(PaintStyle::Stroke);
                    paint.set_stroke_width(1.5);
                    canvas.draw_path(&pb.detach(), &paint);
                    paint.set_style(PaintStyle::Fill);
                }
            }
        }

        canvas.restore(); // end dep list clip

        // Dep list scrollbar
        let total_dep_h = self.dependencies.len() as f32 * DEP_ROW_H;
        let visible_dep_h = dep_list.height();
        let max_dep_scroll = (total_dep_h - visible_dep_h).max(0.0);
        if max_dep_scroll > 0.0 {
            let thumb_h = (visible_dep_h * visible_dep_h / total_dep_h).max(20.0);
            let thumb_y =
                dep_list.top + (self.dep_scroll_y / max_dep_scroll) * (visible_dep_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        dep_list.right - SCROLLBAR_W - 2.0,
                        thumb_y,
                        SCROLLBAR_W,
                        thumb_h,
                    ),
                    2.0,
                    2.0,
                ),
                &paint,
            );
        }

        // Dep plus button
        let dep_plus_rect = Self::dep_plus_rect(width, height);
        paint.set_color(Color::from(if self.hovered_dep_plus {
            TOOLBAR_BTN_HOVER_BG
        } else {
            SUBTLE_BG
        }));
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_color(Color::from(INPUT_BORDER));
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(1.0);
        canvas.draw_rrect(
            RRect::new_rect_xy(dep_plus_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        paint.set_style(PaintStyle::Fill);
        {
            let cx = dep_plus_rect.left + dep_plus_rect.width() / 2.0;
            let cy = dep_plus_rect.top + dep_plus_rect.height() / 2.0;
            let s = 6.0;
            let mut pb = PathBuilder::new();
            pb.move_to((cx - s, cy));
            pb.line_to((cx + s, cy));
            pb.move_to((cx, cy - s));
            pb.line_to((cx, cy + s));
            paint.set_color(Color::from(TOOLBAR_BTN_ICON_COLOR));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
        }

        // Forward dependents (read-only, edit mode only)
        if let Mode::Edit(task_id) = self.mode {
            let fwd_lbl_y = Self::fwd_label_y(width, height);
            if let Some(blob) = TextBlob::new("Required by", &cache.small_font) {
                paint.set_color(Color::from(LABEL_FG));
                canvas.draw_text_blob(&blob, (lx, fwd_lbl_y + lyo), &paint);
            }

            let fwd_list = Self::fwd_list_rect(width, height);
            paint.set_color(Color::from(INPUT_BORDER));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.0);
            canvas.draw_rrect(
                RRect::new_rect_xy(fwd_list, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            paint.set_style(PaintStyle::Fill);

            let node = NodeId::Task(task_id);
            let mut fwd_items: Vec<String> = Vec::new();
            for task in plan.tasks.values() {
                if task.dependencies.iter().any(|d| d.id == node) {
                    fwd_items.push(task.name.clone());
                }
            }
            for ms in plan.milestones.values() {
                if ms.dependencies.iter().any(|d| d.id == node) {
                    fwd_items.push(ms.name.clone());
                }
            }
            fwd_items.sort();

            canvas.save();
            canvas.clip_rect(fwd_list, ClipOp::Intersect, false);

            if fwd_items.is_empty() {
                if let Some(blob) = TextBlob::new("Nothing depends on this task", &cache.small_font)
                {
                    let (_, sm2) = cache.small_font.metrics();
                    let ty =
                        fwd_list.top + (FWD_ROW_H - (sm2.descent - sm2.ascent)) / 2.0 - sm2.ascent;
                    paint.set_color(Color::from(MUTED_FG));
                    canvas.draw_text_blob(&blob, (fwd_list.left + 12.0, ty), &paint);
                }
            } else {
                for (i, name) in fwd_items.iter().enumerate() {
                    if i > 0 {
                        paint.set_color(Color::from(DIVIDER_COLOR));
                        canvas.draw_rect(
                            Rect::from_xywh(
                                fwd_list.left,
                                fwd_list.top + i as f32 * FWD_ROW_H,
                                fwd_list.width(),
                                1.0,
                            ),
                            &paint,
                        );
                    }
                    if let Some(blob) = TextBlob::new(name.as_str(), &cache.small_font) {
                        let (_, sm2) = cache.small_font.metrics();
                        let ty = fwd_list.top
                            + i as f32 * FWD_ROW_H
                            + (FWD_ROW_H - (sm2.descent - sm2.ascent)) / 2.0
                            - sm2.ascent;
                        paint.set_color(Color::from(INPUT_FG));
                        canvas.draw_text_blob(&blob, (fwd_list.left + 12.0, ty), &paint);
                    }
                }
            }
            canvas.restore();
        }

        // Save button
        paint.set_color(Color::from(if self.hovered_save {
            BTN_PRIMARY_HOVER_BG
        } else {
            BTN_PRIMARY_BG
        }));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(save_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
            &paint,
        );
        if let Some(blob) = TextBlob::new("Save", &cache.font) {
            let (_, m) = cache.font.metrics();
            let (adv, _) = cache.font.measure_str("Save", None);
            let tx = save_btn.left + (SAVE_BTN_W - adv) / 2.0;
            let ty = save_btn.top + (PLAN_BTN_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
            paint.set_color(Color::from(BTN_PRIMARY_FG));
            canvas.draw_text_blob(&blob, (tx, ty), &paint);
        }

        // Delete button — only shown when editing an existing task.
        if matches!(self.mode, Mode::Edit(_)) {
            paint.set_color(Color::from(if self.hovered_delete {
                BTN_DANGER_HOVER_BG
            } else {
                BTN_DANGER_BG
            }));
            paint.set_style(PaintStyle::Fill);
            canvas.draw_rrect(
                RRect::new_rect_xy(delete_btn, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
                &paint,
            );
            if let Some(blob) = TextBlob::new("Delete", &cache.font) {
                let (_, m) = cache.font.metrics();
                let (adv, _) = cache.font.measure_str("Delete", None);
                let tx = delete_btn.left + (DELETE_BTN_W - adv) / 2.0;
                let ty = delete_btn.top + (PLAN_BTN_H - (m.descent - m.ascent)) / 2.0 - m.ascent;
                paint.set_color(Color::from(BTN_DANGER_FG));
                canvas.draw_text_blob(&blob, (tx, ty), &paint);
            }
        }

        canvas.restore(); // end content scroll region

        // Scheduler error: red border around the panel + fixed banner below title bar.
        if let Some(ref err_msg) = self.scheduler_error {
            // Red border stroke around the entire panel
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(2.5);
            canvas.draw_rrect(RRect::new_rect_xy(panel, CORNER, CORNER), &paint);
            paint.set_style(PaintStyle::Fill);

            // Error banner below title bar, inset from panel edges to avoid corner clipping.
            const PAD_V: f32 = 8.0;
            let inset = 2.0;
            let banner_x = panel.left + inset;
            let banner_w = panel.width() - 2.0 * inset;
            let max_w = banner_w - 2.0 * PLAN_FORM_PADDING;
            let (_, bm) = cache.small_font.metrics();
            let line_h = (bm.descent - bm.ascent).ceil() + 2.0;
            let lines = wrap_text(err_msg, &cache.small_font, max_w);
            let banner_h = PAD_V * 2.0 + line_h * lines.len() as f32;
            let banner_rect =
                Rect::from_xywh(banner_x, panel.top + TITLE_H + 1.0, banner_w, banner_h);
            paint.set_color(Color::from(ERROR_BG));
            canvas.draw_rect(banner_rect, &paint);
            paint.set_color(Color::from(INPUT_BORDER_ERROR));
            let text_x = banner_x + PLAN_FORM_PADDING;
            let mut text_y = banner_rect.top + PAD_V - bm.ascent;
            for line in &lines {
                if let Some(blob) = TextBlob::new(line.as_str(), &cache.small_font) {
                    canvas.draw_text_blob(&blob, (text_x, text_y), &paint);
                }
                text_y += line_h;
            }
        }

        // Scrollbar
        let content_area_h = panel.height() - TITLE_H - 1.0;
        let full_content_h = PANEL_H - TITLE_H - 1.0;
        let max_scroll = (full_content_h - content_area_h).max(0.0);
        if max_scroll > 0.0 {
            let thumb_h = (content_area_h * content_area_h / full_content_h).max(20.0);
            let thumb_y =
                (panel.top + TITLE_H + 1.0) + (scroll_y / max_scroll) * (content_area_h - thumb_h);
            paint.set_color(Color::from(SCROLLBAR_THUMB_COLOR));
            canvas.draw_rrect(
                RRect::new_rect_xy(
                    Rect::from_xywh(
                        panel.right - SCROLLBAR_W - 2.0,
                        thumb_y,
                        SCROLLBAR_W,
                        thumb_h,
                    ),
                    2.0,
                    2.0,
                ),
                &paint,
            );
        }

        // Calendar popup (on top)
        if let Some(target) = self.open_calendar {
            let trigger = Self::trigger_rect_for(target, width, height);
            let scrolled_trigger = Rect::from_xywh(
                trigger.left,
                trigger.top - scroll_y,
                trigger.width(),
                trigger.height(),
            );
            let cal_rect = Self::calendar_popup_rect(scrolled_trigger, panel);
            let picker = self.picker_ref(target);
            draw_calendar_popup(canvas, cal_rect, picker, today, cache);
        }

        // User/tag picker dropdown (on top of everything)
        if let Some(slot_idx) = self.open_slot_dropdown
            && slot_idx < self.workers.len()
        {
            let list2 = Self::worker_list_rect(width, height);
            // Adjust list top for both form scroll and worker list scroll so dropdown appears at the correct screen position
            let adjusted_list = Rect::from_xywh(
                list2.left,
                list2.top - scroll_y - self.worker_scroll_y,
                list2.width(),
                list2.height(),
            );
            let dd_rect = TaskFormWindow::slot_dropdown_rect(adjusted_list, slot_idx, panel);
            match self.workers[slot_idx].slot_type {
                SlotType::Specific => draw_user_dropdown(
                    canvas,
                    dd_rect,
                    &self.workers[slot_idx],
                    self.slot_dropdown_hovered,
                    self.slot_dropdown_scroll,
                    plan,
                    cache,
                ),
                SlotType::Placeholder => draw_tag_dropdown(
                    canvas,
                    dd_rect,
                    &self.workers[slot_idx],
                    self.slot_dropdown_hovered,
                    self.slot_dropdown_scroll,
                    plan,
                    cache,
                ),
            }
        }

        // Dep dropdown (drawn on top of everything, in screen space)
        if let Some(dep_idx) = self.dep_dropdown_open_for
            && dep_idx < self.dependencies.len()
        {
            let dep_list2 = Self::dep_list_rect(width, height);
            let adjusted_dep_list = Rect::from_xywh(
                dep_list2.left,
                dep_list2.top - scroll_y - self.dep_scroll_y,
                dep_list2.width(),
                dep_list2.height(),
            );
            let dd_rect = TaskFormWindow::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
            draw_dep_dropdown(
                canvas,
                dd_rect,
                &self.dependencies[dep_idx],
                self.dep_dropdown_hovered,
                self.dep_dropdown_scroll,
                self.mode_task_id(),
                plan,
                cache,
            );
        }
    }

    fn on_cursor_moved(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        plan: &Plan,
    ) -> FloatingWindowOutcome {
        let pt = Point::new(x, y);
        let scroll_y = self.effective_scroll(width, height);
        let pt_form = Point::new(x, y + scroll_y);
        let panel = Self::panel_rect(width, height);
        let mut changed = false;

        macro_rules! set {
            ($field:expr, $val:expr) => {{
                let v = $val;
                if $field != v {
                    $field = v;
                    changed = true;
                }
            }};
        }

        set!(
            self.hovered_back,
            Self::back_btn_rect(width, height).contains(pt)
        );
        set!(
            self.hovered_save,
            Self::save_btn_rect(width, height).contains(pt_form)
        );
        set!(
            self.hovered_delete,
            matches!(self.mode, Mode::Edit(_))
                && Self::delete_btn_rect(width, height).contains(pt_form)
        );
        set!(
            self.hovered_plus,
            self.open_calendar.is_none() && Self::worker_plus_rect(width, height).contains(pt_form)
        );

        // Calendar hover
        if let Some(target) = self.open_calendar {
            let trigger = Self::trigger_rect_for(target, width, height);
            let scrolled_trigger = Rect::from_xywh(
                trigger.left,
                trigger.top - scroll_y,
                trigger.width(),
                trigger.height(),
            );
            let cal = Self::calendar_popup_rect(scrolled_trigger, panel);
            let day_1 = first_weekday_offset(
                self.picker_ref(target).nav_year,
                self.picker_ref(target).nav_month,
            );
            let num_days = days_in_month(
                self.picker_ref(target).nav_year,
                self.picker_ref(target).nav_month,
            );

            let new_prev_year = Self::cal_prev_year_btn(cal).contains(pt);
            let new_prev_month = Self::cal_prev_month_btn(cal).contains(pt);
            let new_next_month = Self::cal_next_month_btn(cal).contains(pt);
            let new_next_year = Self::cal_next_year_btn(cal).contains(pt);
            let new_clear = Self::cal_clear_btn(cal).contains(pt);
            let new_today = Self::cal_today_btn(cal).contains(pt);
            let mut new_day: Option<u32> = None;
            for day in 1..=num_days {
                if TaskFormWindow::cal_day_cell(cal, day_1, day).contains(pt) {
                    new_day = Some(day);
                    break;
                }
            }
            let (opy, opm, onm, ony, oc, ot, od) = {
                let p = self.picker_ref(target);
                (
                    p.hovered_prev_year,
                    p.hovered_prev_month,
                    p.hovered_next_month,
                    p.hovered_next_year,
                    p.hovered_clear,
                    p.hovered_today,
                    p.hovered_day,
                )
            };
            if new_prev_year != opy
                || new_prev_month != opm
                || new_next_month != onm
                || new_next_year != ony
                || new_clear != oc
                || new_today != ot
                || new_day != od
            {
                let p = self.picker_mut(target);
                p.hovered_prev_year = new_prev_year;
                p.hovered_prev_month = new_prev_month;
                p.hovered_next_month = new_next_month;
                p.hovered_next_year = new_next_year;
                p.hovered_clear = new_clear;
                p.hovered_today = new_today;
                p.hovered_day = new_day;
                changed = true;
            }
        }

        // User/tag dropdown hover
        if let Some(slot_idx) = self.open_slot_dropdown
            && slot_idx < self.workers.len()
        {
            let list = Self::worker_list_rect(width, height);
            let adjusted_list = Rect::from_xywh(
                list.left,
                list.top - scroll_y - self.worker_scroll_y,
                list.width(),
                list.height(),
            );
            let dd = TaskFormWindow::slot_dropdown_rect(adjusted_list, slot_idx, panel);
            let list_top = dd.top + USER_DROPDOWN_FILTER_H + 1.0;
            let filtered_len = match self.workers[slot_idx].slot_type {
                SlotType::Specific => self.workers[slot_idx].filtered_users(plan).len(),
                SlotType::Placeholder => self.workers[slot_idx].filtered_tags(plan).len(),
            };
            let new_hov = if y >= list_top && x >= dd.left && x <= dd.right {
                let abs =
                    ((y - list_top) / USER_DROPDOWN_ROW_H) as usize + self.slot_dropdown_scroll;
                if abs < filtered_len { Some(abs) } else { None }
            } else {
                None
            };
            set!(self.slot_dropdown_hovered, new_hov);
        }

        // Worker row hovers (user btn, remove, workload)
        let list = Self::worker_list_rect(width, height);
        let in_list = list.contains(pt_form);
        set!(self.cursor_in_worker_list, in_list);
        // Track cursor in description box
        set!(
            self.cursor_in_desc,
            Self::full_input_rect(ROW_DESC, width, height).contains(pt_form)
        );
        if self.open_slot_dropdown.is_none() && self.open_calendar.is_none() {
            // pt_worker: screen pt converted to worker-list content space
            let pt_worker = Point::new(x, y + scroll_y + self.worker_scroll_y);
            for abs in 0..self.workers.len() {
                let new_ub = TaskFormWindow::slot_user_rect(list, abs).contains(pt_worker);
                let new_rm = TaskFormWindow::slot_remove_rect(list, abs).contains(pt_worker);
                let type_rect = TaskFormWindow::slot_type_rect(list, abs);
                let new_type = if type_rect.contains(pt_worker) {
                    let half = type_rect.width() / 2.0;
                    if x < type_rect.left + half {
                        Some(0)
                    } else {
                        Some(1)
                    }
                } else {
                    None
                };
                if self.workers[abs].hovered_user_btn != new_ub {
                    self.workers[abs].hovered_user_btn = new_ub;
                    changed = true;
                }
                if self.workers[abs].hovered_remove != new_rm {
                    self.workers[abs].hovered_remove = new_rm;
                    changed = true;
                }
                if self.workers[abs].hovered_type != new_type {
                    self.workers[abs].hovered_type = new_type;
                    changed = true;
                }
            }
        }

        // Dep dropdown hover
        if let Some(dep_idx) = self.dep_dropdown_open_for
            && dep_idx < self.dependencies.len()
        {
            let dep_list2 = Self::dep_list_rect(width, height);
            let adjusted_dep_list = Rect::from_xywh(
                dep_list2.left,
                dep_list2.top - scroll_y - self.dep_scroll_y,
                dep_list2.width(),
                dep_list2.height(),
            );
            let dd = TaskFormWindow::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
            // rebuild filtered list to compute len
            let dep_list_filtered = {
                let dep_ref = &self.dependencies[dep_idx];
                let filter = dep_ref.dep_filter.content.to_lowercase();
                let mut count = 0usize;
                if filter.is_empty() || "plan start".contains(filter.as_str()) {
                    count += 1;
                }
                for (id, t) in &plan.tasks {
                    if let Mode::Edit(edit_id) = self.mode
                        && *id == edit_id
                    {
                        continue;
                    }
                    if filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                for m in plan.milestones.values() {
                    if filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                count
            };
            let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
            let new_hov = if y >= list_top && x >= dd.left && x <= dd.right {
                let abs = ((y - list_top) / DEP_DROPDOWN_ROW_H) as usize + self.dep_dropdown_scroll;
                if abs < dep_list_filtered {
                    Some(abs)
                } else {
                    None
                }
            } else {
                None
            };
            set!(self.dep_dropdown_hovered, new_hov);
        } else {
            // Dep list hover (target and remove buttons)
            let dep_list2 = Self::dep_list_rect(width, height);
            let in_dep_list = {
                let dep_list_screen = Rect::from_xywh(
                    dep_list2.left,
                    dep_list2.top - scroll_y,
                    dep_list2.width(),
                    dep_list2.height(),
                );
                dep_list_screen.contains(Point::new(x, y))
            };
            set!(self.cursor_in_dep_list, in_dep_list);
            set!(self.hovered_dep_plus, {
                let dep_plus = Self::dep_plus_rect(width, height);
                let dep_plus_screen = Rect::from_xywh(
                    dep_plus.left,
                    dep_plus.top - scroll_y,
                    dep_plus.width(),
                    dep_plus.height(),
                );
                dep_plus_screen.contains(Point::new(x, y))
            });
            let pt_dep = Point::new(x, y + scroll_y + self.dep_scroll_y);
            for dep in &mut self.dependencies {
                dep.hovered_target = false;
                dep.hovered_remove = false;
            }
            for (i, dep) in self.dependencies.iter_mut().enumerate() {
                let target_rect = Self::dep_target_rect(dep_list2, i);
                let remove_rect = Self::dep_remove_rect(dep_list2, i);
                if target_rect.contains(pt_dep) {
                    dep.hovered_target = true;
                    changed = true;
                }
                if remove_rect.contains(pt_dep) {
                    dep.hovered_remove = true;
                    changed = true;
                }
            }
        }

        // Segmented / date trigger hovers
        let new_status = Self::status_btn_rects(width, height)
            .iter()
            .position(|r| r.contains(pt_form));
        let new_ck = Self::constraint_kind_btn_rects(width, height)
            .iter()
            .position(|r| r.contains(pt_form));
        let new_ct = self.constraint_kind != ConstraintSel::None
            && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form);
        let start_disabled = self.status == Status::NotStarted;
        let end_disabled = !matches!(self.status, Status::Complete | Status::Dropped);
        let new_as =
            !start_disabled && Self::left_input_rect(ROW_DATES, width, height).contains(pt_form);
        let new_ae =
            !end_disabled && Self::right_input_rect(ROW_DATES, width, height).contains(pt_form);
        let new_relaxed = Self::relaxed_btn_rect(width, height).contains(pt_form);

        set!(self.hovered_status, new_status);
        set!(self.hovered_constraint_kind, new_ck);
        set!(self.constraint_date.hovered_trigger, new_ct);
        set!(self.actual_start.hovered_trigger, new_as);
        set!(self.actual_end.hovered_trigger, new_ae);
        set!(self.hovered_relaxed, new_relaxed);

        if changed {
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn on_mouse_input(
        &mut self,
        x: f32,
        y: f32,
        pressed: bool,
        width: f32,
        height: f32,
        modifiers: &Modifiers,
        plan: &Plan,
        sender: &PlanRequestSender,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        if !pressed {
            return FloatingWindowOutcome::default();
        }
        let pt = Point::new(x, y);
        let scroll_y = self.effective_scroll(width, height);
        let pt_form = Point::new(x, y + scroll_y);
        let panel = Self::panel_rect(width, height);

        // Deselect any focused text input; specific click targets below will re-focus.
        self.set_focus(TextField::None);

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(pt_form) {
            return self.try_submit(plan, sender);
        }
        if let Mode::Edit(task_id) = self.mode
            && Self::delete_btn_rect(width, height).contains(pt_form)
        {
            sender.send(PlanRequest::DeleteTask(task_id));
            return FloatingWindowOutcome::close();
        }

        // Calendar popup
        if let Some(target) = self.open_calendar {
            let trigger = Self::trigger_rect_for(target, width, height);
            let scrolled_trigger = Rect::from_xywh(
                trigger.left,
                trigger.top - scroll_y,
                trigger.width(),
                trigger.height(),
            );
            let cal = Self::calendar_popup_rect(scrolled_trigger, panel);
            if cal.contains(pt) {
                if TaskFormWindow::cal_prev_year_btn(cal).contains(pt) {
                    self.picker_mut(target).prev_year();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::cal_prev_month_btn(cal).contains(pt) {
                    self.picker_mut(target).prev_month();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::cal_next_month_btn(cal).contains(pt) {
                    self.picker_mut(target).next_month();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::cal_next_year_btn(cal).contains(pt) {
                    self.picker_mut(target).next_year();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::cal_clear_btn(cal).contains(pt) {
                    self.picker_mut(target).value = None;
                    self.close_calendar();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::cal_today_btn(cal).contains(pt) {
                    let today = chrono::Local::now().date_naive();
                    let on_today_month = self.picker_ref(target).nav_year == today.year()
                        && self.picker_ref(target).nav_month == today.month();
                    if on_today_month {
                        // Already showing current month — select today
                        self.picker_mut(target).value = Some(today);
                        if matches!(target, OpenCalendar::Constraint) {
                            self.constraint_date_error = false;
                        }
                        self.close_calendar();
                    } else {
                        // Navigate to current month first
                        let p = self.picker_mut(target);
                        p.nav_year = today.year();
                        p.nav_month = today.month();
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                let day_1 = first_weekday_offset(
                    self.picker_ref(target).nav_year,
                    self.picker_ref(target).nav_month,
                );
                let num_days = days_in_month(
                    self.picker_ref(target).nav_year,
                    self.picker_ref(target).nav_month,
                );
                for day in 1..=num_days {
                    if TaskFormWindow::cal_day_cell(cal, day_1, day).contains(pt) {
                        let p = self.picker_mut(target);
                        p.value = NaiveDate::from_ymd_opt(p.nav_year, p.nav_month, day);
                        if matches!(target, OpenCalendar::Constraint) {
                            self.constraint_date_error = false;
                        }
                        self.close_calendar();
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
            self.close_calendar();
            if !panel.contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // User/tag dropdown
        if let Some(slot_idx) = self.open_slot_dropdown {
            if slot_idx < self.workers.len() {
                let list = Self::worker_list_rect(width, height);
                let adjusted_list = Rect::from_xywh(
                    list.left,
                    list.top - scroll_y - self.worker_scroll_y,
                    list.width(),
                    list.height(),
                );
                let dd = TaskFormWindow::slot_dropdown_rect(adjusted_list, slot_idx, panel);
                if dd.contains(pt) {
                    let filter_rect =
                        Rect::from_xywh(dd.left, dd.top, dd.width(), USER_DROPDOWN_FILTER_H);
                    if filter_rect.contains(pt) {
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    let list_top = dd.top + USER_DROPDOWN_FILTER_H + 1.0;
                    if y >= list_top {
                        let abs = ((y - list_top) / USER_DROPDOWN_ROW_H) as usize
                            + self.slot_dropdown_scroll;
                        match self.workers[slot_idx].slot_type {
                            SlotType::Specific => {
                                let filtered = self.workers[slot_idx].filtered_users(plan);
                                if let Some((uid, _)) = filtered.get(abs) {
                                    let uid = **uid;
                                    self.workers[slot_idx].user_id = Some(uid);
                                    self.worker_error = false;
                                }
                                self.close_slot_dropdown();
                            }
                            SlotType::Placeholder => {
                                let filtered = self.workers[slot_idx].filtered_tags(plan);
                                if let Some(tag) = filtered.get(abs) {
                                    let tag_id = tag.id;
                                    if self.workers[slot_idx].required_tags.contains(&tag_id) {
                                        self.workers[slot_idx].required_tags.remove(&tag_id);
                                    } else {
                                        self.workers[slot_idx].required_tags.insert(tag_id);
                                    }
                                    self.worker_error = false;
                                }
                                // Don't close — allow multi-select
                            }
                        }
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            self.close_slot_dropdown();
            if !panel.contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Dep dropdown
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            if dep_idx < self.dependencies.len() {
                let dep_list2 = Self::dep_list_rect(width, height);
                let adjusted_dep_list = Rect::from_xywh(
                    dep_list2.left,
                    dep_list2.top - scroll_y - self.dep_scroll_y,
                    dep_list2.width(),
                    dep_list2.height(),
                );
                let dd = TaskFormWindow::dep_dropdown_rect(adjusted_dep_list, dep_idx, panel);
                if dd.contains(pt) {
                    let filter_rect =
                        Rect::from_xywh(dd.left, dd.top, dd.width(), DEP_DROPDOWN_FILTER_H);
                    if filter_rect.contains(pt) {
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    let list_top = dd.top + DEP_DROPDOWN_FILTER_H + 1.0;
                    if y >= list_top {
                        let abs = ((y - list_top) / DEP_DROPDOWN_ROW_H) as usize
                            + self.dep_dropdown_scroll;
                        // rebuild filtered list
                        let filter = self.dependencies[dep_idx].dep_filter.content.to_lowercase();
                        let mut items: Vec<(NodeId, String)> = Vec::new();
                        if filter.is_empty() || "plan start".contains(filter.as_str()) {
                            items.push((NodeId::PlanStart, "Plan Start".to_string()));
                        }
                        let edit_task_id = self.mode_task_id();
                        let mut task_items: Vec<(NodeId, String)> = plan
                            .tasks
                            .iter()
                            .filter(|(id, t)| {
                                (edit_task_id != Some(**id))
                                    && (filter.is_empty()
                                        || t.name.to_lowercase().contains(filter.as_str()))
                            })
                            .map(|(id, t)| (NodeId::Task(*id), t.name.clone()))
                            .collect();
                        task_items.sort_by(|a, b| a.1.cmp(&b.1));
                        items.extend(task_items);
                        let mut ms_items: Vec<(NodeId, String)> = plan
                            .milestones
                            .iter()
                            .filter(|(_, m)| {
                                filter.is_empty() || m.name.to_lowercase().contains(filter.as_str())
                            })
                            .map(|(id, m)| (NodeId::Milestone(*id), m.name.clone()))
                            .collect();
                        ms_items.sort_by(|a, b| a.1.cmp(&b.1));
                        items.extend(ms_items);

                        if let Some((node_id, _)) = items.get(abs) {
                            self.dependencies[dep_idx].target = Some(*node_id);
                            self.dep_error = false;
                        }
                        self.close_dep_dropdown();
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            self.close_dep_dropdown();
            if !panel.contains(pt) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Worker list interactions — skip if a calendar popup is open (the popup renders over the workers area)
        let list = Self::worker_list_rect(width, height);
        let plus_rect = Self::worker_plus_rect(width, height);

        if self.open_calendar.is_none() && plus_rect.contains(pt_form) {
            self.workers.push(WorkerSlotEdit::new());
            // Scroll to show the new item
            let total_h = self.workers.len() as f32 * WORKER_ROW_H;
            let visible_h = WORKER_ROW_H * MAX_VISIBLE_WORKERS as f32;
            self.worker_scroll_y = (total_h - visible_h).max(0.0);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if list.contains(pt_form) {
            // pt_worker: form-pt converted to worker-list content space
            let pt_worker = Point::new(x, y + scroll_y + self.worker_scroll_y);
            for abs in 0..self.workers.len() {
                if TaskFormWindow::slot_remove_rect(list, abs).contains(pt_worker) {
                    self.workers.remove(abs);
                    self.clamp_worker_scroll_y();
                    if let Some(ref mut fs) = self.focused_slot_workload {
                        if *fs == abs {
                            self.focused_slot_workload = None;
                        } else if *fs > abs {
                            *fs -= 1;
                        }
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::slot_type_rect(list, abs).contains(pt_worker) {
                    let type_rect = TaskFormWindow::slot_type_rect(list, abs);
                    let new_type = if x < type_rect.left + type_rect.width() / 2.0 {
                        SlotType::Placeholder
                    } else {
                        SlotType::Specific
                    };
                    if self.workers[abs].slot_type != new_type {
                        self.workers[abs].slot_type = new_type;
                        // Reset picker state when switching types
                        self.workers[abs].user_id = None;
                        self.workers[abs].user_filter = TextInput::new("");
                        self.workers[abs].required_tags = HashSet::new();
                        self.workers[abs].tag_filter = TextInput::new("");
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::slot_user_rect(list, abs).contains(pt_worker) {
                    self.open_slot_dropdown(abs);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::slot_workload_rect(list, abs).contains(pt_worker) {
                    self.focused_slot_workload = Some(abs);
                    self.name.focused = false;
                    self.description.focused = false;
                    self.duration.focused = false;
                    let wl_rect = TaskFormWindow::slot_workload_rect(list, abs);
                    let x_in_inner =
                        x - (wl_rect.left + 8.0) + self.workers[abs].workload.scroll_x.get();
                    self.workers[abs].workload.cursor = self.workers[abs]
                        .workload
                        .cursor_for_x(x_in_inner, &cache.font);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Dep list interactions
        let dep_list2 = Self::dep_list_rect(width, height);
        let dep_plus = Self::dep_plus_rect(width, height);

        if dep_plus.contains(pt_form) {
            self.dependencies.push(DependencyEdit::new());
            let total_h = self.dependencies.len() as f32 * DEP_ROW_H;
            let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
            self.dep_scroll_y = (total_h - visible_h).max(0.0);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if dep_list2.contains(pt_form) {
            let pt_dep = Point::new(x, y + scroll_y + self.dep_scroll_y);
            for abs in 0..self.dependencies.len() {
                if TaskFormWindow::dep_remove_rect(dep_list2, abs).contains(pt_dep) {
                    self.dependencies.remove(abs);
                    self.clamp_dep_scroll_y();
                    if let Some(ref mut fl) = self.focused_dep_lag {
                        if *fl == abs {
                            self.focused_dep_lag = None;
                        } else if *fl > abs {
                            *fl -= 1;
                        }
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::dep_target_rect(dep_list2, abs).contains(pt_dep) {
                    self.open_dep_dropdown(abs);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                if TaskFormWindow::dep_lag_rect(dep_list2, abs).contains(pt_dep) {
                    self.focused_dep_lag = Some(abs);
                    self.focused_slot_workload = None;
                    self.name.focused = false;
                    self.description.focused = false;
                    self.duration.focused = false;
                    let lag_rect = TaskFormWindow::dep_lag_rect(dep_list2, abs);
                    let x_in_inner =
                        x - (lag_rect.left + 8.0) + self.dependencies[abs].lag_input.scroll_x.get();
                    self.dependencies[abs].lag_input.cursor = self.dependencies[abs]
                        .lag_input
                        .cursor_for_x(x_in_inner, &cache.font);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Text fields
        // Description (multi-line) handled separately
        let desc_rect = Self::full_input_rect(ROW_DESC, width, height);
        if desc_rect.contains(pt_form) {
            self.set_focus(TextField::Description);
            let x_in_box = x - desc_rect.left;
            let y_in_box = pt_form.y - desc_rect.top;
            let inner_width = desc_rect.width() - 16.0;
            self.description.cursor = self.description.cursor_for_click(
                x_in_box,
                y_in_box,
                inner_width,
                &cache.font,
                DESC_LINE_H,
            );
            // Ctrl+click: open link under cursor
            if modifiers.state().control_key() {
                let cursor = self.description.cursor;
                let content = self.description.content.clone();
                let links = crate::ui::multi_line_input::MultiLineInput::find_links(&content);
                if let Some(range) = links.iter().find(|r| r.contains(&cursor)) {
                    crate::ui::multi_line_input::MultiLineInput::open_url(&content[range.clone()]);
                }
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        for field in [TextField::Name, TextField::Duration] {
            let rect = match field {
                TextField::Name => Self::full_input_rect(ROW_NAME, width, height),
                TextField::Duration => Self::left_input_rect(ROW_DURATION, width, height),
                TextField::None | TextField::Description => unreachable!(),
            };
            if rect.contains(pt_form) {
                self.set_focus(field);
                let inner_left = rect.left + 8.0;
                let x_in_inner = x - inner_left
                    + match field {
                        TextField::Name => self.name.scroll_x.get(),
                        TextField::Duration => self.duration.scroll_x.get(),
                        TextField::None | TextField::Description => unreachable!(),
                    };
                match field {
                    TextField::Name => {
                        self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
                    }
                    TextField::Duration => {
                        self.duration.cursor = self.duration.cursor_for_x(x_in_inner, &cache.font);
                    }
                    TextField::None | TextField::Description => unreachable!(),
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        // Status segmented
        for (i, r) in Self::status_btn_rects(width, height).iter().enumerate() {
            if r.contains(pt_form) {
                let new_status = [
                    Status::NotStarted,
                    Status::InProgress,
                    Status::OnHold,
                    Status::Complete,
                    Status::Dropped,
                ][i];
                let today = chrono::Local::now().date_naive();
                // Auto-populate actual_start when switching to InProgress
                if new_status == Status::InProgress && self.actual_start.value.is_none() {
                    self.actual_start.value = Some(today);
                }
                // Auto-populate actual_end when switching to Complete or Dropped
                if matches!(new_status, Status::Complete | Status::Dropped)
                    && self.actual_end.value.is_none()
                {
                    self.actual_end.value = Some(today);
                }
                self.status = new_status;
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        // Constraint kind
        for (i, r) in Self::constraint_kind_btn_rects(width, height)
            .iter()
            .enumerate()
        {
            if r.contains(pt_form) {
                self.constraint_kind = [
                    ConstraintSel::None,
                    ConstraintSel::Earliest,
                    ConstraintSel::Fixed,
                    ConstraintSel::Latest,
                ][i];
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        // Strict mode toggle
        if Self::relaxed_btn_rect(width, height).contains(pt_form) {
            self.relaxed_mode = !self.relaxed_mode;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Date triggers
        if self.constraint_kind != ConstraintSel::None
            && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form)
        {
            self.open_calendar_picker(OpenCalendar::Constraint);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if self.status != Status::NotStarted
            && Self::left_input_rect(ROW_DATES, width, height).contains(pt_form)
        {
            self.open_calendar_picker(OpenCalendar::ActualStart);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if matches!(self.status, Status::Complete | Status::Dropped)
            && Self::right_input_rect(ROW_DATES, width, height).contains(pt_form)
        {
            self.open_calendar_picker(OpenCalendar::ActualEnd);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        if !panel.contains(pt) {
            return FloatingWindowOutcome::close();
        }
        FloatingWindowOutcome::default()
    }

    fn on_key_input(
        &mut self,
        key: &Key,
        sender: &PlanRequestSender,
        width: f32,
        height: f32,
        plan: &Plan,
        cache: &RenderCache,
    ) -> FloatingWindowOutcome {
        // Calendar open: any key closes it (Escape also closes window)
        if self.open_calendar.is_some() {
            self.close_calendar();
            if *key == Key::Named(NamedKey::Escape) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }

        // Slot dropdown open: route keys to filter input
        if let Some(slot_idx) = self.open_slot_dropdown {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.close_slot_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => {
                    self.close_slot_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Backspace) => {
                    if slot_idx < self.workers.len() {
                        match self.workers[slot_idx].slot_type {
                            SlotType::Specific => self.workers[slot_idx].user_filter.backspace(),
                            SlotType::Placeholder => self.workers[slot_idx].tag_filter.backspace(),
                        }
                        self.slot_dropdown_scroll = 0;
                        self.slot_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    if slot_idx < self.workers.len() {
                        match self.workers[slot_idx].slot_type {
                            SlotType::Specific => {
                                self.workers[slot_idx].user_filter.insert_str(" ")
                            }
                            SlotType::Placeholder => {
                                self.workers[slot_idx].tag_filter.insert_str(" ")
                            }
                        }
                        self.slot_dropdown_scroll = 0;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Character(c) => {
                    if c.chars().all(|ch| !ch.is_control()) && slot_idx < self.workers.len() {
                        match self.workers[slot_idx].slot_type {
                            SlotType::Specific => {
                                self.workers[slot_idx].user_filter.insert_str(c.as_str())
                            }
                            SlotType::Placeholder => {
                                self.workers[slot_idx].tag_filter.insert_str(c.as_str())
                            }
                        }
                        self.slot_dropdown_scroll = 0;
                        self.slot_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Dep dropdown open: route keys to filter input
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            match key {
                Key::Named(NamedKey::Escape) | Key::Named(NamedKey::Enter) => {
                    self.close_dep_dropdown();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Backspace) => {
                    if dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.backspace();
                        self.dep_dropdown_scroll = 0;
                        self.dep_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    if dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.insert_str(" ");
                        self.dep_dropdown_scroll = 0;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Character(c) => {
                    if c.chars().all(|ch| !ch.is_control()) && dep_idx < self.dependencies.len() {
                        self.dependencies[dep_idx].dep_filter.insert_str(c.as_str());
                        self.dep_dropdown_scroll = 0;
                        self.dep_dropdown_hovered = None;
                    }
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Workload input focused
        if let Some(slot_idx) = self.focused_slot_workload
            && slot_idx < self.workers.len()
        {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.focused_slot_workload = None;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Tab) => {
                    self.focused_slot_workload = None;
                    self.set_focus(TextField::Name);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => return self.try_submit(plan, sender),
                Key::Named(NamedKey::Backspace) => {
                    self.workers[slot_idx].workload.backspace();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.workers[slot_idx].workload.move_left();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.workers[slot_idx].workload.move_right();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Home) => {
                    self.workers[slot_idx].workload.move_home();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::End) => {
                    self.workers[slot_idx].workload.move_end();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    return FloatingWindowOutcome::default(); // no spaces in numbers
                }
                Key::Character(c) => {
                    // Only allow numeric chars
                    if c.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
                        self.workers[slot_idx].workload.insert_str(c.as_str());
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    return FloatingWindowOutcome::default();
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Dep lag input focused
        if let Some(lag_idx) = self.focused_dep_lag
            && lag_idx < self.dependencies.len()
        {
            match key {
                Key::Named(NamedKey::Escape) => {
                    self.focused_dep_lag = None;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Tab) => {
                    self.focused_dep_lag = None;
                    self.set_focus(TextField::Name);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Enter) => return self.try_submit(plan, sender),
                Key::Named(NamedKey::Backspace) => {
                    self.dependencies[lag_idx].lag_input.backspace();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.dependencies[lag_idx].lag_input.move_left();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.dependencies[lag_idx].lag_input.move_right();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Home) => {
                    self.dependencies[lag_idx].lag_input.move_home();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::End) => {
                    self.dependencies[lag_idx].lag_input.move_end();
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    return FloatingWindowOutcome::default();
                }
                Key::Character(c) => {
                    if c.chars()
                        .all(|ch| ch.is_ascii_digit() || ch == '.' || ch == '-')
                    {
                        self.dependencies[lag_idx].lag_input.insert_str(c.as_str());
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    return FloatingWindowOutcome::default();
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        // Normal text field routing
        if self.focused == TextField::None {
            if *key == Key::Named(NamedKey::Escape) {
                return FloatingWindowOutcome::close();
            }
            return FloatingWindowOutcome::default();
        }

        // Description (multi-line): Enter inserts newline, not submit
        if self.focused == TextField::Description {
            let desc_rect = Self::full_input_rect(ROW_DESC, width, height);
            let inner_width = desc_rect.width() - 16.0;
            let (_, metrics) = cache.font.metrics();
            let line_h = metrics.descent - metrics.ascent + 2.0;
            let visible_h = DESC_H - 8.0;
            match key {
                Key::Named(NamedKey::Escape) => return FloatingWindowOutcome::close(),
                Key::Named(NamedKey::Enter) => {
                    self.description.insert_newline();
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Tab) => {
                    self.set_focus(TextField::Duration);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Backspace) => {
                    self.description.backspace();
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.description.move_left();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.description.move_right();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.description.move_up(inner_width, &cache.font);
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.description.move_down(inner_width, &cache.font);
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Home) => {
                    self.description.move_to_start();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::End) => {
                    self.description.move_to_end();
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Named(NamedKey::Space) => {
                    self.description.insert_char(' ');
                    self.description.x_hint = None;
                    self.description
                        .scroll_to_cursor(inner_width, &cache.font, line_h, visible_h);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                Key::Character(c) => {
                    if c.chars().all(|ch| !ch.is_control()) {
                        for ch in c.chars() {
                            self.description.insert_char(ch);
                        }
                        self.description.x_hint = None;
                        self.description.scroll_to_cursor(
                            inner_width,
                            &cache.font,
                            line_h,
                            visible_h,
                        );
                        return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                    }
                    return FloatingWindowOutcome::default();
                }
                _ => return FloatingWindowOutcome::default(),
            }
        }

        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => self.try_submit(plan, sender),
            Key::Named(NamedKey::Tab) => {
                let next = match self.focused {
                    TextField::None => TextField::Name,
                    TextField::Name => TextField::Description,
                    TextField::Description => TextField::Duration,
                    TextField::Duration => TextField::Name,
                };
                self.set_focus(next);
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Backspace) => {
                self.focused_input_mut().backspace();
                match self.focused {
                    TextField::Name => self.name_error = false,
                    TextField::Duration => self.duration_error = false,
                    _ => {}
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowLeft) => {
                self.focused_input_mut().move_left();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::ArrowRight) => {
                self.focused_input_mut().move_right();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Home) => {
                self.focused_input_mut().move_home();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::End) => {
                self.focused_input_mut().move_end();
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Named(NamedKey::Space) => {
                self.focused_input_mut().insert_str(" ");
                match self.focused {
                    TextField::Name => self.name_error = false,
                    TextField::Duration => self.duration_error = false,
                    _ => {}
                }
                FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
            }
            Key::Character(c) => {
                if c.chars().all(|ch| !ch.is_control()) {
                    self.focused_input_mut().insert_str(c.as_str());
                    match self.focused {
                        TextField::Name => self.name_error = false,
                        TextField::Duration => self.duration_error = false,
                        _ => {}
                    }
                    FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
                } else {
                    FloatingWindowOutcome::default()
                }
            }
            _ => FloatingWindowOutcome::default(),
        }
    }

    fn on_scroll(
        &mut self,
        delta_y: f32,
        plan: &Plan,
        width: f32,
        height: f32,
    ) -> FloatingWindowOutcome {
        // Scroll user/tag dropdown if open
        if let Some(slot_idx) = self.open_slot_dropdown {
            if slot_idx < self.workers.len() {
                let total = match self.workers[slot_idx].slot_type {
                    SlotType::Specific => self.workers[slot_idx].filtered_users(plan).len(),
                    SlotType::Placeholder => self.workers[slot_idx].filtered_tags(plan).len(),
                };
                let max = total.saturating_sub(MAX_USER_DROPDOWN_ROWS);
                if max == 0 {
                    return FloatingWindowOutcome::default();
                }
                let new_scroll = if delta_y > 0.0 {
                    self.slot_dropdown_scroll.saturating_sub(1)
                } else {
                    (self.slot_dropdown_scroll + 1).min(max)
                };
                if new_scroll != self.slot_dropdown_scroll {
                    self.slot_dropdown_scroll = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::default();
        }

        // Scroll dep dropdown if open
        if let Some(dep_idx) = self.dep_dropdown_open_for {
            if dep_idx < self.dependencies.len() {
                let filter = self.dependencies[dep_idx].dep_filter.content.to_lowercase();
                let mut count = 0usize;
                if filter.is_empty() || "plan start".contains(filter.as_str()) {
                    count += 1;
                }
                for (id, t) in &plan.tasks {
                    if let Mode::Edit(edit_id) = self.mode
                        && *id == edit_id
                    {
                        continue;
                    }
                    if filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                for m in plan.milestones.values() {
                    if filter.is_empty() || m.name.to_lowercase().contains(filter.as_str()) {
                        count += 1;
                    }
                }
                let max = count.saturating_sub(MAX_DEP_DROPDOWN_ROWS);
                if max == 0 {
                    return FloatingWindowOutcome::default();
                }
                let new_scroll = if delta_y > 0.0 {
                    self.dep_dropdown_scroll.saturating_sub(1)
                } else {
                    (self.dep_dropdown_scroll + 1).min(max)
                };
                if new_scroll != self.dep_dropdown_scroll {
                    self.dep_dropdown_scroll = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
            }
            return FloatingWindowOutcome::default();
        }

        // Scroll dep list independently when cursor is inside it
        if self.cursor_in_dep_list {
            let content_h = self.dependencies.len() as f32 * DEP_ROW_H;
            let visible_h = DEP_ROW_H * MAX_VISIBLE_DEPS as f32;
            let max_dscroll = (content_h - visible_h).max(0.0);
            if max_dscroll > 0.0 {
                let new_scroll = (self.dep_scroll_y - delta_y * 40.0).clamp(0.0, max_dscroll);
                if (new_scroll - self.dep_scroll_y).abs() > f32::EPSILON {
                    self.dep_scroll_y = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                return FloatingWindowOutcome::default();
            }
        }

        // Scroll description box independently when cursor is inside it,
        // but only if the description has enough content to scroll.
        if self.cursor_in_desc {
            let max_dscroll = self.max_desc_scroll.get();
            if max_dscroll > 0.0 {
                let cur = self.description.scroll_y.get();
                let new_scroll = (cur - delta_y * 40.0).clamp(0.0, max_dscroll);
                if (new_scroll - cur).abs() > f32::EPSILON {
                    self.description.scroll_y.set(new_scroll);
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                return FloatingWindowOutcome::default();
            }
            // Description not scrollable — fall through to form scroll
        }

        // Scroll worker list independently when cursor is inside it
        if self.cursor_in_worker_list {
            let total_h = self.workers.len() as f32 * WORKER_ROW_H;
            let visible_h = WORKER_ROW_H * MAX_VISIBLE_WORKERS as f32;
            let max_wscroll = (total_h - visible_h).max(0.0);
            if max_wscroll > 0.0 {
                let new_scroll = (self.worker_scroll_y - delta_y * 40.0).clamp(0.0, max_wscroll);
                if (new_scroll - self.worker_scroll_y).abs() > f32::EPSILON {
                    self.worker_scroll_y = new_scroll;
                    return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
                }
                return FloatingWindowOutcome::default();
            }
        }

        // Smooth continuous scroll of the form — same feel as the users list
        let panel_h = Self::panel_rect(width, height).height();
        let max_scroll = (PANEL_H - panel_h).max(0.0);
        if max_scroll <= 0.0 {
            return FloatingWindowOutcome::default();
        }
        let new_scroll = (self.form_scroll_y - delta_y * 40.0).clamp(0.0, max_scroll);
        if (new_scroll - self.form_scroll_y).abs() > f32::EPSILON {
            self.form_scroll_y = new_scroll;
            FloatingWindowOutcome::dirty(DirtyRegion::PageOnly)
        } else {
            FloatingWindowOutcome::default()
        }
    }

    fn reset_hover(&mut self) {
        self.hovered_back = false;
        self.hovered_save = false;
        self.hovered_delete = false;
        self.hovered_plus = false;
        self.hovered_status = None;
        self.hovered_constraint_kind = None;
        self.constraint_date.hovered_trigger = false;
        self.actual_start.hovered_trigger = false;
        self.actual_end.hovered_trigger = false;
        for slot in &mut self.workers {
            slot.hovered_type = None;
            slot.hovered_user_btn = false;
            slot.hovered_remove = false;
        }
        for dep in &mut self.dependencies {
            dep.hovered_target = false;
            dep.hovered_remove = false;
        }
        self.hovered_dep_plus = false;
        self.dep_dropdown_hovered = None;
    }
}
// }}}
