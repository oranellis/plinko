//! Plinko → Monday.com export logic.
//!
//! Writes computed start/end dates, task statuses, and dependency links back
//! to Monday.com.

use std::sync::{Arc, Mutex};

use plinko_shared::data::Plan;
use plinko_shared::data::allocation::{Status, TaskAllocation};
use plinko_shared::data::ids::NodeId;
use plinko_shared::monday::{ItemNodeMapping, MondayConfig};

use crate::monday::client::{MondayApiError, MondayClient};

// ── Diff-based push ────────────────────────────────────────────────────────────

/// A single pending update operation.
struct PushOp {
    board_id: String,
    item_id: String,
    kind: PushKind,
}

enum PushKind {
    Timeline { from: String, to: String },
    Status { label: String },
    Deps { dep_ids: Vec<String> },
}

/// Diff-based export: fetch current Monday state, compute what actually changed,
/// then push only those updates.
///
/// `progress` is updated to `Some((done, total))` once the total is known, and
/// `done` is incremented after every completed operation so callers can display
/// live progress.
pub fn export_to_monday_diff(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
    progress: &Arc<Mutex<Option<(usize, usize)>>>,
) -> Result<String, MondayApiError> {
    let timeline_col = &config.column_map.timeline_column_id;
    let status_col = &config.column_map.status_column_id;
    let dep_col = &config.column_map.dependency_column_id;
    let person_col = &config.column_map.person_column_id;
    let workload_col = &config.column_map.workload_column_id;

    if timeline_col.is_empty() {
        return Err(MondayApiError(
            "Timeline column ID is not configured.".to_string(),
        ));
    }

    // ── Phase 1: fetch current Monday state ───────────────────────────────────
    let monday_items = client.fetch_items(
        &config.board_id,
        person_col,
        status_col,
        dep_col,
        workload_col,
        timeline_col,
    )?;

    let monday_map: std::collections::HashMap<&str, &plinko_shared::monday::MondayItem> =
        monday_items.iter().map(|i| (i.id.as_str(), i)).collect();

    let find_monday_id = |node: &NodeId| -> Option<&str> {
        item_node_map
            .iter()
            .find(|m| &m.plinko_node_id == node)
            .map(|m| m.monday_item_id.as_str())
    };

    let find_label = |status: Status| -> Option<&str> {
        config
            .status_mappings
            .iter()
            .find(|m| m.plinko_status == status)
            .map(|m| m.monday_label.as_str())
    };

    // ── Phase 2: compute diff ─────────────────────────────────────────────────
    let mut ops: Vec<PushOp> = Vec::new();
    let mut skipped = 0usize;

    for mapping in item_node_map {
        let monday_item_id = &mapping.monday_item_id;
        let board_id = if mapping.board_id.is_empty() {
            &config.board_id
        } else {
            &mapping.board_id
        };
        let current = monday_map.get(monday_item_id.as_str());

        // Timeline
        let timeline = match &mapping.plinko_node_id {
            NodeId::Task(task_id) => {
                let Some(state) = plan.node_allocations.tasks.get(task_id) else {
                    skipped += 1;
                    continue;
                };
                let start = state.allocation.start_date();
                let end = match &state.allocation {
                    TaskAllocation::Fixed {
                        corrected_end_date: Some(c),
                        ..
                    } => *c,
                    _ => state.allocation.end_date(),
                };
                Some((
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                ))
            }
            NodeId::Milestone(ms_id) => {
                let Some(ms_alloc) = plan.node_allocations.milestones.get(ms_id) else {
                    skipped += 1;
                    continue;
                };
                let d = ms_alloc.date().format("%Y-%m-%d").to_string();
                Some((d.clone(), d))
            }
            NodeId::PlanStart => {
                skipped += 1;
                continue;
            }
        };

        if let Some((from, to)) = timeline {
            let needs_update = current.map_or(true, |item| {
                let cur_from = item
                    .timeline_start
                    .map(|d| d.format("%Y-%m-%d").to_string());
                let cur_to = item.timeline_end.map(|d| d.format("%Y-%m-%d").to_string());
                cur_from.as_deref() != Some(&from) || cur_to.as_deref() != Some(&to)
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.clone(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Timeline { from, to },
                });
            }
        }

        // Status
        if !status_col.is_empty() {
            let plinko_status = match &mapping.plinko_node_id {
                NodeId::Task(task_id) => Some(plan.task_status(task_id)),
                NodeId::Milestone(ms_id) => plan
                    .node_allocations
                    .milestones
                    .get(ms_id)
                    .map(|ma| ma.derived_status()),
                NodeId::PlanStart => None,
            };
            if let Some(label) = plinko_status.and_then(find_label) {
                let needs_update =
                    current.map_or(true, |item| item.status_label.as_deref() != Some(label));
                if needs_update {
                    ops.push(PushOp {
                        board_id: board_id.clone(),
                        item_id: monday_item_id.clone(),
                        kind: PushKind::Status {
                            label: label.to_string(),
                        },
                    });
                }
            }
        }

        // Dependencies
        if !dep_col.is_empty() {
            let deps = plan.get_dependencies(&mapping.plinko_node_id);
            let mut plinko_dep_ids: Vec<String> = deps
                .iter()
                .filter_map(|d| find_monday_id(&d.id).map(|s| s.to_string()))
                .collect();
            plinko_dep_ids.sort();

            let needs_update = current.map_or(true, |item| {
                let mut cur_deps = item.dependency_item_ids.clone();
                cur_deps.sort();
                cur_deps != plinko_dep_ids
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.clone(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Deps {
                        dep_ids: plinko_dep_ids,
                    },
                });
            }
        }
    }

    // ── Phase 3: execute ops with progress tracking ───────────────────────────
    let total = ops.len();
    *progress.lock().unwrap() = Some((0, total));

    if total == 0 {
        return Ok(format!(
            "Nothing to update ({skipped} items skipped — already up to date)."
        ));
    }

    let mut updated = 0usize;
    let mut failed = 0usize;

    for (i, op) in ops.into_iter().enumerate() {
        let result = match op.kind {
            PushKind::Timeline { from, to } => {
                client.update_timeline(&op.board_id, &op.item_id, timeline_col, &from, &to)
            }
            PushKind::Status { label } => {
                client.update_status(&op.board_id, &op.item_id, status_col, &label)
            }
            PushKind::Deps { dep_ids } => {
                let dep_refs: Vec<&str> = dep_ids.iter().map(|s| s.as_str()).collect();
                client.update_dependencies(&op.board_id, &op.item_id, dep_col, &dep_refs)
            }
        };
        match result {
            Ok(()) => updated += 1,
            Err(e) => {
                eprintln!("Warning: push op failed for {}: {e}", op.item_id);
                failed += 1;
            }
        }
        *progress.lock().unwrap() = Some((i + 1, total));
    }

    if failed == 0 {
        Ok(format!(
            "Push complete: {updated} updated, {skipped} skipped."
        ))
    } else {
        Ok(format!(
            "Push complete: {updated} updated, {failed} failed, {skipped} skipped."
        ))
    }
}

/// Export scheduled dates, statuses, and dependencies to Monday.com.
///
/// For each item in `item_node_map`:
/// - Updates the timeline column with the computed start/end dates.
/// - Updates the status column using the configured status label mappings.
/// - Updates the dependency column to reflect plinko dependencies mapped to
///   Monday item IDs (non-mappable deps such as PlanStart are omitted).
///
/// Returns a status message.
pub fn export_to_monday(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
) -> Result<String, MondayApiError> {
    let timeline_col = &config.column_map.timeline_column_id;
    let status_col = &config.column_map.status_column_id;
    let dep_col = &config.column_map.dependency_column_id;

    if timeline_col.is_empty() {
        return Err(MondayApiError(
            "Timeline column ID is not configured.".to_string(),
        ));
    }

    // Closure to look up the Monday item ID for a given plinko NodeId.
    let find_monday_id = |node: &NodeId| -> Option<&str> {
        item_node_map
            .iter()
            .find(|m| &m.plinko_node_id == node)
            .map(|m| m.monday_item_id.as_str())
    };

    // Build reverse status lookup: plinko Status → Monday label.
    // If multiple mappings share the same status (shouldn't happen), first wins.

    let find_label = |status: Status| -> Option<&str> {
        config
            .status_mappings
            .iter()
            .find(|m| m.plinko_status == status)
            .map(|m| m.monday_label.as_str())
    };

    let mut updated = 0usize;
    let mut skipped = 0usize;

    for mapping in item_node_map {
        let monday_item_id = &mapping.monday_item_id;
        // Use the item's own board_id (set during import for subitems); fall
        // back to the plan-level board_id for entries saved before this field
        // was added.
        let board_id = if mapping.board_id.is_empty() {
            &config.board_id
        } else {
            &mapping.board_id
        };

        // ── Timeline ─────────────────────────────────────────────────────────
        let timeline_result = match &mapping.plinko_node_id {
            NodeId::Task(task_id) => {
                let Some(state) = plan.node_allocations.tasks.get(task_id) else {
                    skipped += 1;
                    continue;
                };
                let start = state.allocation.start_date();
                let end = match &state.allocation {
                    TaskAllocation::Fixed {
                        corrected_end_date: Some(c),
                        ..
                    } => *c,
                    _ => state.allocation.end_date(),
                };
                Some((
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                ))
            }
            NodeId::Milestone(ms_id) => {
                let Some(ms_alloc) = plan.node_allocations.milestones.get(ms_id) else {
                    skipped += 1;
                    continue;
                };
                let d = ms_alloc.date().format("%Y-%m-%d").to_string();
                Some((d.clone(), d))
            }
            NodeId::PlanStart => {
                skipped += 1;
                continue;
            }
        };

        if let Some((from, to)) = timeline_result {
            match client.update_timeline(board_id, monday_item_id, timeline_col, &from, &to) {
                Ok(()) => updated += 1,
                Err(e) => {
                    eprintln!("Warning: failed to update timeline for {monday_item_id}: {e}");
                    skipped += 1;
                }
            }
        }

        // ── Status ───────────────────────────────────────────────────────────
        if !status_col.is_empty() {
            let plinko_status = match &mapping.plinko_node_id {
                NodeId::Task(task_id) => Some(plan.task_status(task_id)),
                NodeId::Milestone(ms_id) => plan
                    .node_allocations
                    .milestones
                    .get(ms_id)
                    .map(|ma| ma.derived_status()),
                NodeId::PlanStart => None,
            };
            if let Some(label) = plinko_status.and_then(find_label)
                && let Err(e) = client.update_status(board_id, monday_item_id, status_col, label)
            {
                eprintln!("Warning: failed to update status for {monday_item_id}: {e}");
            }
        }

        // ── Dependencies ─────────────────────────────────────────────────────
        if !dep_col.is_empty() {
            let deps = plan.get_dependencies(&mapping.plinko_node_id);
            let monday_dep_ids: Vec<&str> =
                deps.iter().filter_map(|d| find_monday_id(&d.id)).collect();
            if let Err(e) =
                client.update_dependencies(board_id, monday_item_id, dep_col, &monday_dep_ids)
            {
                eprintln!("Warning: failed to update dependencies for {monday_item_id}: {e}");
            }
        }
    }

    Ok(format!(
        "Export complete: {updated} updated, {skipped} skipped."
    ))
}
