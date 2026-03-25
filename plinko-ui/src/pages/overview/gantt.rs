//! Gantt chart layout helpers: row packing and date-range computation.

use chrono::NaiveDate;
use std::collections::HashMap;

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
/// The bar start is the earliest `WorkSegment` date across all workers — i.e.
/// the first day anyone actually works on the task. The bar end is the latest
/// work segment date or the allocation end date, whichever is later.
///
/// Falls back to stored `allocation.start_date()` / `end_date()` when no work
/// segments exist (e.g. task has no workers yet).
///
/// Returns `None` if no allocation information is available at all.
pub fn task_display_dates(plan: &Plan, id: &TaskId) -> Option<(NaiveDate, NaiveDate)> {
    let task = plan.tasks.get(id)?;

    let ts = plan.node_allocations.tasks.get(id)?;

    let segs = match &ts.allocation {
        plinko_shared::data::TaskAllocation::Dynamic {
            time_allocation, ..
        }
        | plinko_shared::data::TaskAllocation::Fixed {
            time_allocation, ..
        } => time_allocation.as_slice(),
    };

    let first_seg = segs.iter().map(|s| s.date).min();
    let last_seg = segs.iter().map(|s| s.date).max();

    // For InProgress tasks with an actual_start, the bar should begin from that
    // date even if the earliest segment is in the future.
    let start = if let Some(actual) = task.actual_start {
        first_seg.map_or(actual, |f| f.min(actual))
    } else {
        first_seg.unwrap_or_else(|| ts.allocation.start_date())
    };
    let end = match last_seg {
        Some(last) => last.max(ts.allocation.end_date()),
        None => ts.allocation.end_date(),
    };
    // InProgress tasks should always extend to at least today on the Gantt
    // chart, even if all their allocated work is in the past.
    let end = if ts.status == Status::InProgress {
        let today = chrono::Local::now().date_naive();
        end.max(today)
    } else {
        end
    };

    Some((start, end))
}

/// Pack all tasks and milestones with known dates into rows using a virtual
/// day-column grid, guaranteeing no visual overlaps.
///
/// Items are sorted by dependency chain depth first, so items that depend on
/// many others appear lower in the chart. Milestones are placed before tasks
/// at the same depth so they act as visual anchors. Within the same depth,
/// items are sorted by start date.
///
/// The plan-start diamond is always placed in row 0 first, so tasks/milestones
/// that start on or near the plan start date are pushed to later rows or
/// positions that don't overlap it.
pub fn pack_rows(plan: &Plan) -> Vec<GanttRow> {
    let ref_date = plan.start_date;
    let depths = compute_topo_depths(plan);

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
    // Sort: milestones before tasks at same depth, then by depth, then by start date.
    items.sort_by(|a, b| {
        let depth_a = match a {
            GanttItem::Task { id, .. } => depths.get(&NodeId::Task(*id)).copied().unwrap_or(0),
            GanttItem::Milestone { id, .. } => {
                depths.get(&NodeId::Milestone(*id)).copied().unwrap_or(0)
            }
            GanttItem::PlanStart { .. } => 0,
        };
        let depth_b = match b {
            GanttItem::Task { id, .. } => depths.get(&NodeId::Task(*id)).copied().unwrap_or(0),
            GanttItem::Milestone { id, .. } => {
                depths.get(&NodeId::Milestone(*id)).copied().unwrap_or(0)
            }
            GanttItem::PlanStart { .. } => 0,
        };
        let ms_a = matches!(a, GanttItem::Milestone { .. });
        let ms_b = matches!(b, GanttItem::Milestone { .. });
        // Stable tiebreaker using the node's UUID so the row assignment is
        // identical for the same plan state, regardless of HashMap iteration order.
        let id_a = match a {
            GanttItem::Task { id, .. } => id.0.to_string(),
            GanttItem::Milestone { id, .. } => id.0.to_string(),
            GanttItem::PlanStart { .. } => String::new(),
        };
        let id_b = match b {
            GanttItem::Task { id, .. } => id.0.to_string(),
            GanttItem::Milestone { id, .. } => id.0.to_string(),
            GanttItem::PlanStart { .. } => String::new(),
        };
        // Milestones go first at the same depth.
        depth_a
            .cmp(&depth_b)
            .then(ms_b.cmp(&ms_a))
            .then(a.start().cmp(&b.start()))
            .then(id_a.cmp(&id_b))
    });

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

/// Compute the topological depth of each node from the plan start.
///
/// Depth 0 = directly depends only on PlanStart or has no deps.
/// Depth N = max(predecessor depths) + 1.
fn compute_topo_depths(plan: &Plan) -> HashMap<NodeId, usize> {
    let mut depths: HashMap<NodeId, usize> = HashMap::new();
    // Iterative BFS / memoised DFS using a work stack.
    fn depth_of(node: NodeId, plan: &Plan, memo: &mut HashMap<NodeId, usize>) -> usize {
        if let Some(&d) = memo.get(&node) {
            return d;
        }
        let deps: Vec<NodeId> = match node {
            NodeId::Task(id) => plan
                .tasks
                .get(&id)
                .map(|t| t.dependencies.iter().map(|d| d.id).collect())
                .unwrap_or_default(),
            NodeId::Milestone(id) => plan
                .milestones
                .get(&id)
                .map(|m| m.dependencies.iter().map(|d| d.id).collect())
                .unwrap_or_default(),
            NodeId::PlanStart => Vec::new(),
        };
        let d = deps
            .into_iter()
            .filter(|dep| *dep != node) // avoid self-loops (shouldn't exist)
            .map(|dep| depth_of(dep, plan, memo) + 1)
            .max()
            .unwrap_or(0);
        memo.insert(node, d);
        d
    }

    for id in plan.tasks.keys() {
        depth_of(NodeId::Task(*id), plan, &mut depths);
    }
    for id in plan.milestones.keys() {
        depth_of(NodeId::Milestone(*id), plan, &mut depths);
    }
    depths
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
