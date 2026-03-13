//! Gantt chart layout helpers: row packing and date-range computation.

use chrono::NaiveDate;

use crate::data::Plan;
use crate::data::ids::{MilestoneId, NodeId, TaskId};
use crate::data::task::TaskStatus;

/// The x-coordinate in "day units" of one item's end, used for row packing.
const ROW_GAP_DAYS: i64 = 1; // minimum gap between items on the same row

// ── GanttItem ─────────────────────────────────────────────────────────────────

/// A single item placed on the Gantt chart.
#[derive(Clone, Debug)]
pub enum GanttItem {
    /// A task bar spanning `start..=end` (inclusive).
    Task {
        id: TaskId,
        start: NaiveDate,
        end: NaiveDate,
    },
    /// A milestone diamond at a specific date.
    Milestone { id: MilestoneId, date: NaiveDate },
}

impl GanttItem {
    /// Inclusive start date of the item for row-packing purposes.
    pub fn start(&self) -> NaiveDate {
        match self {
            GanttItem::Task { start, .. } => *start,
            GanttItem::Milestone { date, .. } => *date,
        }
    }

    /// Inclusive end date of the item for row-packing purposes.
    ///
    /// Milestones add a 2-day visual buffer after their date so the diamond
    /// (which extends `GANTT_MS_HALF` pixels either side of centre) never
    /// overlaps a task bar placed in the same row, even at minimum zoom.
    pub fn end(&self) -> NaiveDate {
        match self {
            GanttItem::Task { end, .. } => *end,
            GanttItem::Milestone { date, .. } => *date + chrono::Duration::days(2),
        }
    }
}

// ── GanttRow ──────────────────────────────────────────────────────────────────

/// A horizontal row on the Gantt chart containing non-overlapping items.
#[derive(Clone, Debug, Default)]
pub struct GanttRow {
    pub items: Vec<GanttItem>,
}

// ── Row packing ───────────────────────────────────────────────────────────────

/// Resolve the display date range for a task.
///
/// Priority:
/// 1. `actual_start_date`..`actual_end_date` (both set → completed)
/// 2. `actual_start_date` + scheduler allocation end (in-progress)
/// 3. Scheduler allocation start/end (most accurate — accounts for weekends)
/// 4. Scheduled start + computed duration (fallback)
/// 5. `None` if no date information is available
pub fn task_display_dates(plan: &Plan, id: &TaskId) -> Option<(NaiveDate, NaiveDate)> {
    use chrono::Duration;

    let task = plan.tasks.get(id)?;
    let duration = task.effective_duration_days().max(1.0) as i64;

    if let (Some(s), Some(e)) = (task.actual_start_date, task.actual_end_date) {
        return Some((s, e));
    }
    if let Some(s) = task.actual_start_date {
        let end = plan
            .allocation
            .as_ref()
            .and_then(|a| a.tasks.get(id))
            .map(|a| a.end_date)
            .unwrap_or_else(|| s + Duration::days(duration - 1));
        return Some((s, end));
    }
    // Prefer allocation dates: these correctly account for weekends and daily-cap spreading.
    if let Some(alloc) = plan.allocation.as_ref().and_then(|a| a.tasks.get(id)) {
        return Some((alloc.start_date, alloc.end_date));
    }
    if let Some(s) = plan.dates.task(id) {
        return Some((s, s + Duration::days(duration - 1)));
    }
    None
}

/// Pack all tasks and milestones with known dates into rows, minimising the
/// number of rows by greedily placing each item in the first row that has
/// enough space (greedy interval-scheduling / earliest-deadline-first).
pub fn pack_rows(plan: &Plan) -> Vec<GanttRow> {
    // Build the flat list of items sorted by start date.
    let mut items: Vec<GanttItem> = Vec::new();

    for id in plan.tasks.keys() {
        if let Some((start, end)) = task_display_dates(plan, id) {
            items.push(GanttItem::Task {
                id: *id,
                start,
                end,
            });
        }
    }

    for id in plan.milestones.keys() {
        if let Some(date) = plan.dates.milestone(id) {
            items.push(GanttItem::Milestone { id: *id, date });
        }
    }

    items.sort_by_key(|i| i.start());

    // Greedy row assignment: for each item find the first row whose last item
    // ends strictly before `item.start - ROW_GAP_DAYS`, else add a new row.
    let mut rows: Vec<GanttRow> = Vec::new();
    // Track the end date of the last item in each row.
    let mut row_ends: Vec<NaiveDate> = Vec::new();

    for item in items {
        let item_start = item.start();
        // Find first row where there's enough gap.
        let target = row_ends.iter().position(|&end| {
            let gap = (item_start - end).num_days();
            gap > ROW_GAP_DAYS
        });

        match target {
            Some(idx) => {
                row_ends[idx] = item.end();
                rows[idx].items.push(item);
            }
            None => {
                row_ends.push(item.end());
                rows.push(GanttRow { items: vec![item] });
            }
        }
    }

    rows
}

// ── Date range ────────────────────────────────────────────────────────────────

/// Returns the earliest start date and latest end date across all tasks and
/// milestones that have computed dates, or `None` if the plan has nothing scheduled.
pub fn compute_date_range(plan: &Plan) -> Option<(NaiveDate, NaiveDate)> {
    use chrono::Duration;

    let mut min_date: Option<NaiveDate> = None;
    let mut max_date: Option<NaiveDate> = None;

    for id in plan.tasks.keys() {
        if let Some((start, end)) = task_display_dates(plan, id) {
            min_date = Some(min_date.map_or(start, |m: NaiveDate| m.min(start)));
            max_date = Some(max_date.map_or(end, |m: NaiveDate| m.max(end)));
        }
    }

    for id in plan.milestones.keys() {
        if let Some(date) = plan.dates.milestone(id) {
            min_date = Some(min_date.map_or(date, |m: NaiveDate| m.min(date)));
            max_date = Some(max_date.map_or(date, |m: NaiveDate| m.max(date)));
        }
    }

    // Always include plan start_date.
    let plan_start = plan.start_date;
    min_date = Some(min_date.map_or(plan_start, |m| m.min(plan_start)));
    max_date = Some(max_date.map_or(plan_start, |m| m.max(plan_start)));

    // Add a small margin so items near the edge aren't clipped.
    let start = min_date? - Duration::days(2);
    let end = max_date? + Duration::days(7);

    Some((start, end))
}

// ── Milestone status ──────────────────────────────────────────────────────────

/// Aggregate milestone color category based on the status of tasks that
/// immediately depend on this milestone (i.e. tasks whose dependency list
/// contains this milestone's id).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MilestoneStatus {
    /// All immediate successors are NotStarted or Dropped.
    NotStarted,
    /// At least one immediate successor is InProgress or OnHold.
    InProgress,
    /// All immediate successors are Complete or Dropped (or there are none).
    Complete,
}

/// Compute the display status of a milestone based on its immediate successor tasks.
pub fn milestone_display_status(plan: &Plan, id: MilestoneId) -> MilestoneStatus {
    let node = NodeId::Milestone(id);
    let mut any_active = false;
    let mut all_done = true;
    let mut found_any = false;

    for task in plan.tasks.values() {
        if task.dependencies.iter().any(|d| d.id == node) {
            found_any = true;
            match task.status {
                TaskStatus::Complete | TaskStatus::Dropped => {}
                TaskStatus::InProgress | TaskStatus::OnHold => {
                    any_active = true;
                    all_done = false;
                }
                TaskStatus::NotStarted => {
                    all_done = false;
                }
            }
        }
    }

    if !found_any || all_done {
        MilestoneStatus::Complete
    } else if any_active {
        MilestoneStatus::InProgress
    } else {
        MilestoneStatus::NotStarted
    }
}
