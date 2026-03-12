//! Floating form for creating or editing a task — all task fields.

use chrono::{Datelike, NaiveDate};
use skia_safe::{
    Canvas, ClipOp, Color, Contains, Paint, PaintStyle, PathBuilder, Point, RRect, Rect, TextBlob,
};
use winit::keyboard::{Key, NamedKey};

use crate::data::constraint::{ConstraintKind, DateConstraint};
use crate::data::ids::{TagId, UserId};
use crate::data::task::{Task, TaskStatus, WorkerSlot};
use crate::data::{Plan, TaskId};
use crate::engine::{PlanRequest, PlanRequestSender, TaskPatch};
use crate::ui::cache::RenderCache;
use crate::ui::dirty::DirtyRegion;
use crate::ui::floating_window::{FloatingWindow, FloatingWindowOutcome};
use crate::ui::layout::{
    BACK_BTN_CORNER, BACK_BTN_HOVER_BG, BACK_BTN_ICON_COLOR, BACK_BTN_SIZE, BTN_PRIMARY_BG,
    BTN_PRIMARY_FG, BTN_SECONDARY_BG, BTN_SECONDARY_FG, DIVIDER_COLOR, INPUT_BG, INPUT_BORDER,
    INPUT_BORDER_ERROR, INPUT_BORDER_FOCUS, INPUT_CURSOR_COLOR, INPUT_FG, ITEM_FG, LABEL_FG,
    LIST_BG, LIST_ITEM_HOVER_BG, PANEL_BG, PLAN_BTN_CORNER, PLAN_BTN_H, PLAN_FIELD_GAP,
    PLAN_FORM_PADDING, PLAN_INPUT_H, PLAN_LABEL_GAP, TOOLBAR_STROKE_WIDTH,
};
use crate::ui::text_input::TextInput;
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
    + FIELD_BLOCK_H   // description
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
    + 20.0
    + PLAN_BTN_H
    + PLAN_FORM_PADDING;

const SCROLLBAR_W: f32 = 4.0;

// ── Helper types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum TextField {
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

    fn filtered_users<'a>(&self, plan: &'a Plan) -> Vec<(&'a UserId, &'a crate::data::User)> {
        let filter = self.user_filter.content.to_lowercase();
        plan.users
            .iter()
            .filter(|(_, u)| filter.is_empty() || u.name.to_lowercase().contains(filter.as_str()))
            .collect::<Vec<_>>()
            .tap_sort_by(|(_, a), (_, b)| a.name.cmp(&b.name))
    }

    fn filtered_tags<'a>(&self, plan: &'a Plan) -> Vec<&'a crate::data::plan::Tag> {
        let filter = self.tag_filter.content.to_lowercase();
        plan.tags
            .iter()
            .filter(|t| filter.is_empty() || t.name.to_lowercase().contains(filter.as_str()))
            .collect()
    }
}

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

// ── Mode ──────────────────────────────────────────────────────────────────────

enum Mode {
    New,
    Edit(TaskId),
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct TaskFormWindow {
    mode: Mode,
    name: TextInput,
    description: TextInput,
    duration: TextInput,
    focused: TextField,
    status: TaskStatus,
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
    name_error: bool,
    duration_error: bool,
    // Buttons
    hovered_back: bool,
    hovered_save: bool,
    // Scroll
    form_scroll_y: f32,
}

impl TaskFormWindow {
    pub fn new() -> Self {
        let mut name = TextInput::new("");
        name.focused = true;
        Self {
            mode: Mode::New,
            name,
            description: TextInput::new(""),
            duration: TextInput::new(""),
            focused: TextField::Name,
            status: TaskStatus::NotStarted,
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
            name_error: false,
            duration_error: false,
            hovered_back: false,
            hovered_save: false,
            form_scroll_y: 0.0,
        }
    }

    pub fn from_task(task: &Task) -> Self {
        let mut name = TextInput::new(&task.name);
        name.focused = true;
        let dur_str = if task.duration_days_target > 0.0 {
            format_days(task.duration_days_target)
        } else {
            String::new()
        };
        let (constraint_kind, constraint_val) = ConstraintSel::from_opt(task.constraint);
        let workers = task.workers.iter().map(WorkerSlotEdit::from_slot).collect();
        Self {
            mode: Mode::Edit(task.id),
            name,
            description: TextInput::new(&task.description),
            duration: TextInput::new(&dur_str),
            focused: TextField::Name,
            status: task.status,
            hovered_status: None,
            constraint_kind,
            hovered_constraint_kind: None,
            constraint_date: CalendarPicker::new(constraint_val),
            actual_start: CalendarPicker::new(task.actual_start_date),
            actual_end: CalendarPicker::new(task.actual_end_date),
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
            name_error: false,
            duration_error: false,
            hovered_back: false,
            hovered_save: false,
            form_scroll_y: 0.0,
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

    fn form_top(width: f32, height: f32) -> f32 {
        Self::panel_rect(width, height).top + TITLE_H + 1.0 + PLAN_FORM_PADDING
    }

    fn row_label_y(row: usize, width: f32, height: f32) -> f32 {
        Self::form_top(width, height) + row as f32 * (FIELD_BLOCK_H + PLAN_FIELD_GAP)
    }

    fn full_input_rect(row: usize, width: f32, height: f32) -> Rect {
        let p = Self::panel_rect(width, height);
        let x = p.left + PLAN_FORM_PADDING;
        let w = p.width() - 2.0 * PLAN_FORM_PADDING;
        let y = Self::row_label_y(row, width, height) + LABEL_H + PLAN_LABEL_GAP;
        Rect::from_xywh(x, y, w, PLAN_INPUT_H)
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
        Self::form_top(width, height) + 6.0 * (FIELD_BLOCK_H + PLAN_FIELD_GAP)
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
    }

    fn focused_input_mut(&mut self) -> &mut TextInput {
        match self.focused {
            TextField::Name => &mut self.name,
            TextField::Description => &mut self.description,
            TextField::Duration => &mut self.duration,
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

    fn clamp_worker_scroll_y(&mut self) {
        let total_h = self.workers.len() as f32 * WORKER_ROW_H;
        let visible_h = WORKER_ROW_H * MAX_VISIBLE_WORKERS as f32;
        let max = (total_h - visible_h).max(0.0);
        self.worker_scroll_y = self.worker_scroll_y.clamp(0.0, max);
    }

    // ── Submit ────────────────────────────────────────────────────────────────

    fn try_submit(&mut self, sender: &PlanRequestSender) -> FloatingWindowOutcome {
        let name = self.name.content.trim().to_string();
        if name.is_empty() {
            self.name_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        // Validate duration is numeric when provided
        if !self.duration.content.trim().is_empty()
            && self.duration.content.trim().parse::<f32>().is_err()
        {
            self.duration_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        // Validate: at least one complete (user-assigned) worker
        let worker_slots: Vec<WorkerSlot> = self
            .workers
            .iter()
            .filter_map(|s| s.to_worker_slot())
            .collect();
        if worker_slots.is_empty() {
            self.worker_error = true;
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        self.worker_error = false;

        let description = self.description.content.trim().to_string();
        let duration: f32 = self
            .duration
            .content
            .trim()
            .parse::<f32>()
            .unwrap_or(0.0)
            .max(0.0);
        let constraint = self
            .constraint_kind
            .to_constraint(self.constraint_date.value);

        match self.mode {
            Mode::New => {
                let mut task = Task::new(name, description);
                task.status = self.status;
                task.duration_days_target = duration;
                task.constraint = constraint;
                task.actual_start_date = self.actual_start.value;
                task.actual_end_date = self.actual_end.value;
                task.workers = worker_slots;
                sender.send(PlanRequest::CreateTask(task));
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
                    .workers(worker_slots);
                sender.send(PlanRequest::UpdateTask(id, patch));
            }
        }
        FloatingWindowOutcome::close()
    }
}

// ── Drawing helpers ───────────────────────────────────────────────────────────

fn draw_chevron_btn(canvas: &Canvas, btn_rect: Rect, hovered: bool) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    if hovered {
        paint.set_color(Color::from(BACK_BTN_HOVER_BG));
        paint.set_style(PaintStyle::Fill);
        canvas.draw_rrect(
            RRect::new_rect_xy(btn_rect, BACK_BTN_CORNER, BACK_BTN_CORNER),
            &paint,
        );
    }
    let cx = btn_rect.left + BACK_BTN_SIZE / 2.0;
    let cy = btn_rect.top + BACK_BTN_SIZE / 2.0;
    let aw = BACK_BTN_SIZE * 0.3;
    let ah = BACK_BTN_SIZE * 0.3;
    let mut pb = PathBuilder::new();
    pb.move_to((cx + aw / 2.0, cy - ah / 2.0));
    pb.line_to((cx - aw / 2.0, cy));
    pb.line_to((cx + aw / 2.0, cy + ah / 2.0));
    paint.set_color(Color::from(BACK_BTN_ICON_COLOR));
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(TOOLBAR_STROKE_WIDTH);
    canvas.draw_path(&pb.detach(), &paint);
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
    cache: &RenderCache,
) {
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let rrect = RRect::new_rect_xy(rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER);
    paint.set_color(if disabled {
        Color::from(0xff_f5f5f5_u32)
    } else {
        Color::from(INPUT_BG)
    });
    paint.set_style(PaintStyle::Fill);
    canvas.draw_rrect(rrect, &paint);
    paint.set_color(if disabled {
        Color::from(0xff_e0e0e0_u32)
    } else if is_open {
        Color::from(INPUT_BORDER_FOCUS)
    } else if picker.hovered_trigger {
        Color::from(0xff_aaaaaa_u32)
    } else {
        Color::from(INPUT_BORDER)
    });
    paint.set_style(PaintStyle::Stroke);
    paint.set_stroke_width(1.0);
    canvas.draw_rrect(rrect, &paint);
    paint.set_style(PaintStyle::Fill);

    let text = picker.display_text();
    if let Some(blob) = TextBlob::new(&text, &cache.font) {
        let (_, m) = cache.font.metrics();
        let ty = rect.top + (rect.height() - (m.descent - m.ascent)) / 2.0 - m.ascent;
        paint.set_color(if disabled {
            Color::from(0xff_cccccc_u32)
        } else if picker.value.is_some() {
            Color::from(INPUT_FG)
        } else {
            Color::from(0xff_aaaaaa_u32)
        });
        canvas.draw_text_blob(&blob, (rect.left + 8.0, ty), &paint);
    }

    // Tiny calendar icon
    let icon_cx = rect.right - 16.0;
    let icon_cy = rect.top + rect.height() / 2.0;
    let hs = 5.0;
    paint.set_color(if disabled {
        Color::from(0xff_cccccc_u32)
    } else {
        Color::from(0xff_999999_u32)
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
            0xff_e0e0e0_u32
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

    paint.set_color(Color::from_argb(35, 0, 0, 0));
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
            0xff_e0e0e0_u32
        } else {
            0xff_f7f7f7_u32
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
            paint.set_color(Color::from(0xff_e8eef8_u32));
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
        0xff_e0e0e0_u32
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
        0xff_e0e0e0_u32
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
                0xff_e0e0e0_u32
            } else {
                0xff_f5f5f5_u32
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
        Color::from(0xff_aaaaaa_u32)
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
            .and_then(|id| plan.users.get(&id))
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
        _ => Color::from(0xff_aaaaaa_u32),
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
        paint.set_color(Color::from(0xff_888888_u32));
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

    // Remove button
    let rm_bg = if slot.hovered_remove {
        0xff_e53935_u32
    } else {
        0xff_f0f0f0_u32
    };
    paint.set_color(Color::from(rm_bg));
    canvas.draw_rrect(
        RRect::new_rect_xy(rm_rect, PLAN_BTN_CORNER, PLAN_BTN_CORNER),
        &paint,
    );
    {
        let cx = rm_rect.left + rm_rect.width() / 2.0;
        let cy = rm_rect.top + rm_rect.height() / 2.0;
        let s = 4.0;
        let mut pb = PathBuilder::new();
        pb.move_to((cx - s, cy - s));
        pb.line_to((cx + s, cy + s));
        pb.move_to((cx + s, cy - s));
        pb.line_to((cx - s, cy + s));
        paint.set_color(if slot.hovered_remove {
            Color::WHITE
        } else {
            Color::from(0xff_888888_u32)
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
    paint.set_color(Color::from_argb(30, 0, 0, 0));
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
            paint.set_color(Color::from(0xff_aaaaaa_u32));
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
    paint.set_color(Color::from_argb(30, 0, 0, 0));
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
            paint.set_color(Color::from(0xff_aaaaaa_u32));
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

// ── FloatingWindow impl ───────────────────────────────────────────────────────

impl FloatingWindow for TaskFormWindow {
    fn render(&self, canvas: &Canvas, width: f32, height: f32, cache: &RenderCache, plan: &Plan) {
        let panel = Self::panel_rect(width, height);
        let back_btn = Self::back_btn_rect(width, height);
        let save_btn = Self::save_btn_rect(width, height);
        let today = chrono::Local::now().date_naive();
        let scroll_y = self.effective_scroll(width, height);

        let mut paint = Paint::default();
        paint.set_anti_alias(true);

        // Drop shadow
        paint.set_color(Color::from_argb(40, 0, 0, 0));
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

        draw_chevron_btn(canvas, back_btn, self.hovered_back);

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
        draw_text_input(
            canvas,
            Self::full_input_rect(ROW_DESC, width, height),
            &self.description,
            self.focused == TextField::Description,
            false,
            cache,
        );

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
            TaskStatus::NotStarted => 0,
            TaskStatus::InProgress => 1,
            TaskStatus::OnHold => 2,
            TaskStatus::Complete => 3,
            TaskStatus::Dropped => 4,
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
            p2.set_color(Color::from(0xff_f0f0f0_u32));
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
                cache,
            );
        }

        // Actual dates
        let start_disabled = self.status == TaskStatus::NotStarted;
        let end_disabled = !matches!(self.status, TaskStatus::Complete | TaskStatus::Dropped);
        label!(ROW_DATES, 0, "Actual Start");
        draw_date_btn(
            canvas,
            Self::left_input_rect(ROW_DATES, width, height),
            &self.actual_start,
            self.open_calendar == Some(OpenCalendar::ActualStart),
            start_disabled,
            cache,
        );
        label!(ROW_DATES, 1, "Actual End");
        draw_date_btn(
            canvas,
            Self::right_input_rect(ROW_DATES, width, height),
            &self.actual_end,
            self.open_calendar == Some(OpenCalendar::ActualEnd),
            end_disabled,
            cache,
        );

        // Workers section
        let wl_y = Self::workers_label_y(width, height);
        let label_text = if self.worker_error {
            "Workers (at least one required)"
        } else {
            "Workers"
        };
        if let Some(blob) = TextBlob::new(label_text, &cache.small_font) {
            paint.set_color(Color::from(if self.worker_error {
                0xff_e53935_u32
            } else {
                LABEL_FG
            }));
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
                paint.set_color(Color::from(0xff_aaaaaa_u32));
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
            paint.set_color(Color::from_argb(80, 0, 0, 0));
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
            0xff_e0e0e0_u32
        } else {
            0xff_f5f5f5_u32
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
            paint.set_color(Color::from(0xff_555555_u32));
            paint.set_style(PaintStyle::Stroke);
            paint.set_stroke_width(1.5);
            canvas.draw_path(&pb.detach(), &paint);
            paint.set_style(PaintStyle::Fill);
        }

        // Save button
        paint.set_color(Color::from(if self.hovered_save {
            0xff_3a7bc8_u32
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

        canvas.restore(); // end content scroll region

        // Scrollbar
        let content_area_h = panel.height() - TITLE_H - 1.0;
        let full_content_h = PANEL_H - TITLE_H - 1.0;
        let max_scroll = (full_content_h - content_area_h).max(0.0);
        if max_scroll > 0.0 {
            let thumb_h = (content_area_h * content_area_h / full_content_h).max(20.0);
            let thumb_y =
                (panel.top + TITLE_H + 1.0) + (scroll_y / max_scroll) * (content_area_h - thumb_h);
            paint.set_color(Color::from_argb(80, 0, 0, 0));
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
            self.hovered_plus,
            Self::worker_plus_rect(width, height).contains(pt_form)
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

        // Segmented / date trigger hovers
        let new_status = Self::status_btn_rects(width, height)
            .iter()
            .position(|r| r.contains(pt_form));
        let new_ck = Self::constraint_kind_btn_rects(width, height)
            .iter()
            .position(|r| r.contains(pt_form));
        let new_ct = self.constraint_kind != ConstraintSel::None
            && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form);
        let start_disabled = self.status == TaskStatus::NotStarted;
        let end_disabled = !matches!(self.status, TaskStatus::Complete | TaskStatus::Dropped);
        let new_as =
            !start_disabled && Self::left_input_rect(ROW_DATES, width, height).contains(pt_form);
        let new_ae =
            !end_disabled && Self::right_input_rect(ROW_DATES, width, height).contains(pt_form);

        set!(self.hovered_status, new_status);
        set!(self.hovered_constraint_kind, new_ck);
        set!(self.constraint_date.hovered_trigger, new_ct);
        set!(self.actual_start.hovered_trigger, new_as);
        set!(self.actual_end.hovered_trigger, new_ae);

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

        if Self::back_btn_rect(width, height).contains(pt) {
            return FloatingWindowOutcome::close();
        }
        if Self::save_btn_rect(width, height).contains(pt_form) {
            return self.try_submit(sender);
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
                    self.picker_mut(target).value = Some(chrono::Local::now().date_naive());
                    self.close_calendar();
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

        // Worker list interactions
        let list = Self::worker_list_rect(width, height);
        let plus_rect = Self::worker_plus_rect(width, height);

        if plus_rect.contains(pt_form) {
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

        // Text fields
        for field in [TextField::Name, TextField::Description, TextField::Duration] {
            let rect = match field {
                TextField::Name => Self::full_input_rect(ROW_NAME, width, height),
                TextField::Description => Self::full_input_rect(ROW_DESC, width, height),
                TextField::Duration => Self::left_input_rect(ROW_DURATION, width, height),
            };
            if rect.contains(pt_form) {
                self.set_focus(field);
                let inner_left = rect.left + 8.0;
                let x_in_inner = x - inner_left
                    + match field {
                        TextField::Name => self.name.scroll_x.get(),
                        TextField::Description => self.description.scroll_x.get(),
                        TextField::Duration => self.duration.scroll_x.get(),
                    };
                match field {
                    TextField::Name => {
                        self.name.cursor = self.name.cursor_for_x(x_in_inner, &cache.font);
                    }
                    TextField::Description => {
                        self.description.cursor =
                            self.description.cursor_for_x(x_in_inner, &cache.font);
                    }
                    TextField::Duration => {
                        self.duration.cursor = self.duration.cursor_for_x(x_in_inner, &cache.font);
                    }
                }
                return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
            }
        }

        // Status segmented
        for (i, r) in Self::status_btn_rects(width, height).iter().enumerate() {
            if r.contains(pt_form) {
                self.status = [
                    TaskStatus::NotStarted,
                    TaskStatus::InProgress,
                    TaskStatus::OnHold,
                    TaskStatus::Complete,
                    TaskStatus::Dropped,
                ][i];
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

        // Date triggers
        if self.constraint_kind != ConstraintSel::None
            && Self::right_input_rect(ROW_CONSTRAINT, width, height).contains(pt_form)
        {
            self.open_calendar_picker(OpenCalendar::Constraint);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if self.status != TaskStatus::NotStarted
            && Self::left_input_rect(ROW_DATES, width, height).contains(pt_form)
        {
            self.open_calendar_picker(OpenCalendar::ActualStart);
            return FloatingWindowOutcome::dirty(DirtyRegion::PageOnly);
        }
        if matches!(self.status, TaskStatus::Complete | TaskStatus::Dropped)
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

    fn on_key_input(&mut self, key: &Key, sender: &PlanRequestSender) -> FloatingWindowOutcome {
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
                Key::Named(NamedKey::Enter) => return self.try_submit(sender),
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

        // Normal text field routing
        match key {
            Key::Named(NamedKey::Escape) => FloatingWindowOutcome::close(),
            Key::Named(NamedKey::Enter) => self.try_submit(sender),
            Key::Named(NamedKey::Tab) => {
                let next = match self.focused {
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
    }
}
