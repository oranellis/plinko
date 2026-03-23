//! Gantt chart layout helpers: row packing and date-range computation.

use chrono::NaiveDate;

use plinko_shared::data::Plan;
use plinko_shared::data::Status;
use plinko_shared::data::ids::{MilestoneId, NodeId, TaskId};

/// Minimum gap in day-units between items that may share a row.
const ROW_GAP_DAYS: i64 = 2;

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
    /// The fixed plan-start diamond (always row 0, always teal).
    PlanStart { date: NaiveDate },
}

// ── Implementation ──────────────────────────────────────────────────────────── {{{
impl GanttItem {
    /// Inclusive start date of the item for row-packing purposes.
    pub fn start(&self) -> NaiveDate {
        match self {
            GanttItem::Task { start, .. } => *start,
            GanttItem::Milestone { date, .. } | GanttItem::PlanStart { date } => *date,
        }
    }

    /// Inclusive end date of the item for row-packing purposes.
    ///
    /// Milestones and the plan-start diamond add a 2-day visual buffer after
    /// their date so the diamond (which extends `GANTT_MS_HALF` pixels either
    /// side of centre) never overlaps a task bar placed in the same row, even
    /// at minimum zoom.
    pub fn end(&self) -> NaiveDate {
        match self {
            GanttItem::Task { end, .. } => *end,
            GanttItem::Milestone { date, .. } | GanttItem::PlanStart { date } => {
                *date + chrono::Duration::days(2)
            }
        }
    }
}
// }}}

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

    if let (Some(s), Some(e)) = (plan.task_actual_start(id), plan.task_actual_end(id)) {
        return Some((s, e));
    }
    if let Some(s) = plan.task_actual_start(id) {
        let end = plan
            .node_allocations
            .tasks
            .get(id)
            .map(|ts| ts.allocation.end_date())
            .unwrap_or_else(|| s + Duration::days(duration - 1));
        return Some((s, end));
    }
    // Prefer allocation dates: these correctly account for weekends and daily-cap spreading.
    if let Some(ts) = plan.node_allocations.tasks.get(id) {
        return Some((ts.allocation.start_date(), ts.allocation.end_date()));
    }
    let _ = task;
    None
}

/// Pack all tasks and milestones with known dates into rows using a virtual
/// day-column grid, guaranteeing no visual overlaps.
///
/// The plan-start diamond is always placed in row 0 first, so tasks/milestones
/// that start on or near the plan start date are pushed to later rows or
/// positions that don't overlap it.
pub fn pack_rows(plan: &Plan) -> Vec<GanttRow> {
    let ref_date = plan.start_date;

    // Virtual grid: for each row, store all occupied intervals as
    // (start_day_offset, end_day_offset_inclusive_with_gap).
    // Using day offsets from ref_date for compact integer arithmetic.
    let mut grid: Vec<Vec<(i64, i64)>> = Vec::new();

    // Reserve row 0 for the plan-start diamond.
    let ps_start = 0_i64;
    let ps_end_with_gap = 2 + ROW_GAP_DAYS; // 2-day diamond buffer + gap
    grid.push(vec![(ps_start, ps_end_with_gap)]);
    let plan_start_item = GanttItem::PlanStart {
        date: plan.start_date,
    };
    let mut rows: Vec<GanttRow> = vec![GanttRow {
        items: vec![plan_start_item],
    }];

    // Collect all schedulable items.
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
        if let Some(date) = plan.node_allocations.milestones.get(id).map(|ma| ma.date()) {
            items.push(GanttItem::Milestone { id: *id, date });
        }
    }
    items.sort_by_key(|i| i.start());

    for item in items {
        let item_start = (item.start() - ref_date).num_days();
        // Include the gap buffer in the end so adjacent items get ROW_GAP_DAYS space.
        let item_end_with_gap = (item.end() - ref_date).num_days() + ROW_GAP_DAYS;

        // Find the first row with no overlapping interval.
        let target = grid.iter().position(|row_intervals| {
            row_intervals
                .iter()
                .all(|&(s, e)| item_end_with_gap < s || item_start > e)
        });

        match target {
            Some(idx) => {
                grid[idx].push((item_start, item_end_with_gap));
                rows[idx].items.push(item);
            }
            None => {
                grid.push(vec![(item_start, item_end_with_gap)]);
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
        if let Some(date) = plan.node_allocations.milestones.get(id).map(|ma| ma.date()) {
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
/// Compute the display status of a milestone based on its predecessor dependencies
/// (the tasks/milestones this milestone depends on).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MilestoneStatus {
    /// All predecessor dependencies are Complete or Dropped (or there are none).
    NotStarted,
    /// At least one predecessor is InProgress or OnHold.
    InProgress,
    /// All predecessor dependencies are Complete or Dropped.
    Complete,
}

/// Compute the display status of a milestone based on its predecessor dependencies
/// (the tasks/milestones this milestone depends on).
pub fn milestone_display_status(plan: &Plan, id: MilestoneId) -> MilestoneStatus {
    let milestone = match plan.milestones.get(&id) {
        Some(m) => m,
        None => return MilestoneStatus::NotStarted,
    };

    if milestone.dependencies.is_empty() {
        return MilestoneStatus::Complete;
    }

    let mut any_active = false;
    let mut all_done = true;

    for dep in &milestone.dependencies {
        match dep.id {
            NodeId::Task(tid) => match plan.task_status(&tid) {
                Status::Complete | Status::Dropped => {}
                Status::InProgress | Status::OnHold => {
                    any_active = true;
                    all_done = false;
                }
                Status::NotStarted => {
                    all_done = false;
                }
            },
            NodeId::Milestone(mid) => {
                let sub = milestone_display_status(plan, mid);
                match sub {
                    MilestoneStatus::Complete => {}
                    MilestoneStatus::InProgress => {
                        any_active = true;
                        all_done = false;
                    }
                    MilestoneStatus::NotStarted => {
                        all_done = false;
                    }
                }
            }
            NodeId::PlanStart => {
                // PlanStart is always "complete" — no contribution.
            }
        }
    }

    if all_done {
        MilestoneStatus::Complete
    } else if any_active {
        MilestoneStatus::InProgress
    } else {
        MilestoneStatus::NotStarted
    }
}
