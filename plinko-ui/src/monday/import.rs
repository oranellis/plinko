//! Monday.com → plinko import logic.
//!
//! Builds the full plan state in memory from Monday data and sends a single
//! `PlanRequest::ReplacePlan` at the end. This avoids intermediate scheduler
//! runs with an invalid (partial) dependency graph.

use std::collections::HashMap;

use chrono::NaiveDate;

use plinko_shared::data::Dependency;
use plinko_shared::data::Plan;
use plinko_shared::data::allocation::Status;
use plinko_shared::data::ids::{MilestoneId, NodeId, TaskId};
use plinko_shared::data::milestone::Milestone;
use plinko_shared::data::task::{Task, WorkerSlot};
use plinko_shared::monday::{ItemNodeMapping, MondayConfig, MondayItem};
use plinko_shared::protocol::PlanRequest;

use crate::engine::PlanRequestSender;
use crate::monday::client::{MondayApiError, MondayClient};

/// Import Monday board items into the plan.
///
/// `plan` is a **clone** of the current plan. All new tasks/milestones are
/// built into it directly (no per-request scheduler runs), then a single
/// `ReplacePlan` is sent at the end.
///
/// Returns updated `item_node_map` entries and a status message.
pub fn import_from_monday(
    client: &MondayClient,
    config: &MondayConfig,
    mut plan: Plan,
    sender: &PlanRequestSender,
) -> Result<(Vec<ItemNodeMapping>, String), MondayApiError> {
    let cm = &config.column_map;

    // Determine the effective board ID for items at the level we're importing.
    // Subitems live on their own board; fetch its ID when use_subitems is true.
    let item_board_id: String = if config.use_subitems {
        client
            .fetch_subitem_board_id(&config.board_id)
            .unwrap_or_else(|_| config.board_id.clone())
    } else {
        config.board_id.clone()
    };

    let all_items = client.fetch_items(
        &config.board_id,
        &cm.person_column_id,
        &cm.status_column_id,
        &cm.dependency_column_id,
        &cm.workload_column_id,
        &cm.timeline_column_id,
    )?;

    // Filter to only the level the user wants (subitems or top-level items).
    let items: Vec<&MondayItem> = all_items
        .iter()
        .filter(|item| {
            if config.use_subitems {
                item.parent_id.is_some()
            } else {
                item.parent_id.is_none()
            }
        })
        .collect();

    // Build a lookup from Monday item ID → existing plinko node ID.
    let existing: HashMap<String, NodeId> = config
        .item_node_map
        .iter()
        .map(|m| (m.monday_item_id.clone(), m.plinko_node_id))
        .collect();

    // Mutable map of Monday item ID → plinko node ID (accumulates new entries).
    let mut id_map: HashMap<String, NodeId> = existing.clone();

    // Track status and timeline dates for each task ID so we can apply them in pass 3.
    let mut task_statuses: HashMap<TaskId, (Status, Option<NaiveDate>, Option<NaiveDate>)> =
        HashMap::new();

    let mut created = 0usize;
    let mut updated = 0usize;

    // ── Pass 1: create or update all nodes in the plan ────────────────────────
    for item in &items {
        let ctx = if config.show_monday_context {
            item.context_label.clone()
        } else {
            None
        };
        if let Some(node_id) = existing.get(&item.id) {
            // Update existing task workers/workload.
            match node_id {
                NodeId::Task(task_id) => {
                    let (workers, _) = build_workers_and_days(item, config);
                    if let Some(task) = plan.tasks.get_mut(task_id) {
                        task.workers = workers;
                        task.name = item.name.clone();
                        task.context_label = ctx;
                    }
                    let status = resolve_status(item, config);
                    task_statuses
                        .insert(*task_id, (status, item.timeline_start, item.timeline_end));
                }
                NodeId::Milestone(ms_id) => {
                    if let Some(ms) = plan.milestones.get_mut(ms_id) {
                        ms.name = item.name.clone();
                        ms.context_label = ctx;
                    }
                }
                NodeId::PlanStart => {}
            }
            updated += 1;
        } else {
            let node_id = if item.is_milestone {
                // Don't import dropped milestones.
                if resolve_status(item, config) == Status::Dropped {
                    continue;
                }
                let mut ms = Milestone::new(&item.name, "");
                ms.context_label = ctx;
                let ms_id = ms.id;
                plan.add_milestone(ms);
                NodeId::Milestone(ms_id)
            } else {
                // Skip tasks with no person assigned and no workload — they're empty placeholders.
                let has_person = !item.assigned_user_ids.is_empty();
                let has_workload = item.workload.is_some_and(|w| w > 0.0);
                if !has_person && !has_workload {
                    continue;
                }
                let mut task = build_task(item, config);
                task.context_label = ctx;
                let task_id = task.id;
                let status = resolve_status(item, config);
                task_statuses.insert(task_id, (status, item.timeline_start, item.timeline_end));
                plan.add_task(task);
                NodeId::Task(task_id)
            };
            id_map.insert(item.id.clone(), node_id);
            created += 1;
        }
    }

    // ── Adjust plan start date ────────────────────────────────────────────────
    // If any item has a timeline_start before the plan's start_date, move it back
    // so that historical tasks don't end up before plan start.
    let earliest_timeline: Option<NaiveDate> =
        items.iter().filter_map(|item| item.timeline_start).min();
    if let Some(earliest) = earliest_timeline
        && earliest < plan.start_date
    {
        plan.start_date = earliest;
    }

    // ── Pass 2: wire dependencies ─────────────────────────────────────────────
    // Tasks/milestones whose Monday deps don't resolve to any known node (or have
    // no deps at all) get PlanStart as a fallback so they always have a path to
    // the plan root and the scheduler can compute a date for them.
    for item in &items {
        let Some(this_node) = id_map.get(&item.id).cloned() else {
            continue;
        };

        let mut resolved: Vec<Dependency> = item
            .dependency_item_ids
            .iter()
            .filter_map(|dep_monday_id| {
                let dep_node = id_map.get(dep_monday_id)?;
                Some(Dependency::new(*dep_node))
            })
            .collect();

        // Fallback: no deps or none resolved → anchor to PlanStart.
        if resolved.is_empty() {
            resolved.push(Dependency::new(NodeId::PlanStart));
        }

        match this_node {
            NodeId::Task(task_id) => {
                for dep in resolved {
                    let _ = plan.add_task_dependency(task_id, dep);
                }
            }
            NodeId::Milestone(ms_id) => {
                for dep in resolved {
                    let _ = plan.add_milestone_dependency(ms_id, dep);
                }
            }
            NodeId::PlanStart => {}
        }
    }

    // ── Pass 3: apply task statuses with timeline dates ───────────────────────
    // Done after dep wiring so InProgress/Complete tasks have valid allocations.
    let plan_start = plan.start_date;
    for (task_id, (status, tl_start, tl_end)) in &task_statuses {
        let has_timeline = tl_start.is_some();

        // For finished tasks (Complete/Dropped) without a timeline: anchor at plan
        // start with a 1-day duration so they don't show the 1970 sentinel.
        // For incomplete tasks (NotStarted/InProgress/OnHold) without a timeline:
        // derive a sensible duration estimate: ceil(2 * total_workload / #workers).
        if !has_timeline && let Some(task) = plan.tasks.get_mut(task_id) {
            match status {
                Status::Complete | Status::Dropped => {
                    // No timeline available — use a 1-day placeholder so we
                    // don't show the 1970 sentinel.
                    task.duration_days_target = 1.0;
                }
                Status::NotStarted | Status::InProgress | Status::OnHold => {
                    let total_workload: f32 = task.workers.iter().map(|w| w.workload_days()).sum();
                    let num_workers = task.workers.len().max(1) as f32;
                    task.duration_days_target =
                        (2.0 * total_workload / num_workers).ceil().max(1.0);
                }
            }
        }

        let start_date = tl_start.unwrap_or(plan_start);
        match status {
            Status::NotStarted => {
                // Overwrite duration from Monday's timeline so a timeline change
                // (e.g. 8 days → 4 days) is reflected on re-import.
                if has_timeline {
                    let wd = timeline_working_days(*tl_start, *tl_end);
                    if wd > 0.0
                        && let Some(task) = plan.tasks.get_mut(task_id)
                    {
                        task.duration_days_target = wd;
                    }
                }
            }
            Status::InProgress => {
                // Set actual_start from timeline then start the task.
                if let Some(task) = plan.tasks.get_mut(task_id) {
                    task.actual_start = Some(start_date);
                }
                plan.start_task(*task_id);
            }
            Status::OnHold => {
                if let Some(task) = plan.tasks.get_mut(task_id) {
                    task.actual_start = Some(start_date);
                }
                plan.start_task(*task_id);
                plan.pause_task(*task_id);
            }
            Status::Complete => {
                if let Some(task) = plan.tasks.get_mut(task_id) {
                    task.actual_start = Some(start_date);
                }
                plan.start_task(*task_id);
                plan.complete_task(*task_id);
                // Use timeline end if available; otherwise end = start (1-day span).
                let end_date = tl_end.unwrap_or(start_date);
                plan.set_task_actual_end(*task_id, Some(end_date));
            }
            Status::Dropped => {
                // Set actual_start so start_task creates a Fixed allocation before we drop.
                // Without this, drop_task leaves the Dynamic allocation with the 1970 sentinel.
                if let Some(task) = plan.tasks.get_mut(task_id) {
                    task.actual_start = Some(start_date);
                    // Apply timeline span so the dropped bar shows its real calendar width.
                    if let Some(end) = tl_end {
                        let span = (*end - start_date).num_days().max(0) as f32 + 1.0;
                        task.duration_days_target = span;
                    }
                }
                plan.start_task(*task_id);
                plan.drop_task(*task_id);
                let end_date = tl_end.unwrap_or(start_date);
                plan.set_task_actual_end(*task_id, Some(end_date));
            }
        }
    }

    // ── Pass 4: run the scheduler once on the complete plan ───────────────────
    plan.simplify_all_dependencies();
    let _ = plan.compute_time_optimised_plan();

    // Send the fully-built plan to the engine in one atomic operation.
    sender.send(PlanRequest::ReplacePlan(Box::new(plan)));

    // Build updated item_node_map.
    let new_map: Vec<ItemNodeMapping> = id_map
        .into_iter()
        .map(|(monday_item_id, plinko_node_id)| ItemNodeMapping {
            monday_item_id,
            plinko_node_id,
            board_id: item_board_id.clone(),
        })
        .collect();

    let message = format!("Import complete: {created} created, {updated} updated.");
    Ok((new_map, message))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_task(item: &MondayItem, config: &MondayConfig) -> Task {
    let (workers, _) = build_workers_and_days(item, config);
    let duration_days_target = timeline_working_days(item.timeline_start, item.timeline_end);
    Task {
        id: TaskId::new(),
        name: item.name.clone(),
        description: String::new(),
        dependencies: Vec::new(),
        workers,
        constraint: None,
        duration_days_target,
        relaxed_mode: false,
        actual_start: None,
        context_label: None,
    }
}

/// Count working days (Mon–Fri) between two dates inclusive.
/// Returns 0.0 if dates are missing or end < start.
fn timeline_working_days(start: Option<NaiveDate>, end: Option<NaiveDate>) -> f32 {
    use chrono::Datelike;
    use chrono::Weekday;
    let (Some(s), Some(e)) = (start, end) else {
        return 0.0;
    };
    if e < s {
        return 0.0;
    }
    let mut days = 0.0_f32;
    let mut cur = s;
    while cur <= e {
        match cur.weekday() {
            Weekday::Sat | Weekday::Sun => {}
            _ => days += 1.0,
        }
        cur = cur.succ_opt().unwrap_or(e);
    }
    days.max(1.0)
}

/// Returns the worker slots and per-worker workload days.
fn build_workers_and_days(item: &MondayItem, config: &MondayConfig) -> (Vec<WorkerSlot>, f32) {
    let plinko_users = build_worker_ids(item, config);
    let workload = item.workload.unwrap_or(1.0);
    let total_days = if config.workload_in_hours {
        workload / 8.0
    } else {
        workload
    };
    let per_worker_days = if plinko_users.is_empty() {
        total_days
    } else {
        total_days / plinko_users.len() as f32
    };

    let workers = if plinko_users.is_empty() {
        vec![WorkerSlot::Placeholder {
            required_tags: Default::default(),
            workload_days: total_days,
        }]
    } else {
        plinko_users
            .into_iter()
            .map(|uid| WorkerSlot::Specific {
                user_id: uid,
                workload_days: per_worker_days,
            })
            .collect()
    };

    (workers, per_worker_days)
}

fn build_worker_ids(
    item: &MondayItem,
    config: &MondayConfig,
) -> Vec<plinko_shared::data::ids::UserId> {
    item.assigned_user_ids
        .iter()
        .filter_map(|monday_uid| {
            config
                .user_mappings
                .iter()
                .find(|m| &m.monday_user_id == monday_uid)
                .and_then(|m| m.plinko_user_id)
        })
        .collect()
}

fn resolve_status(item: &MondayItem, config: &MondayConfig) -> Status {
    let Some(label) = &item.status_label else {
        return Status::NotStarted;
    };
    config
        .status_mappings
        .iter()
        .find(|m| &m.monday_label == label)
        .map(|m| m.plinko_status)
        .unwrap_or(Status::NotStarted)
}
