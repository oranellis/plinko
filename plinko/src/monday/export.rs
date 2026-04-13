//! Plinko → Monday.com export logic.
//!
//! Writes computed start/end dates, task statuses, and dependency links back
//! to Monday.com.

use std::collections::HashMap;

use plinko_shared::data::Plan;
use plinko_shared::data::allocation::{Status, TaskAllocation};
use plinko_shared::data::ids::NodeId;
use plinko_shared::monday::{ItemNodeMapping, MondayConfig};

use crate::monday::client::{MondayApiError, MondayClient};

const FROM_PLINKO_GROUP: &str = "From Plinko";
const FROM_PLINKO_PARENT: &str = "From Plinko";

// ── Diff-based push ────────────────────────────────────────────────────────────

/// A single pending update operation.
struct PushOp {
    board_id: String,
    item_id: String,
    kind: PushKind,
}

enum PushKind {
    Timeline {
        from: String,
        to: String,
        is_milestone: bool,
    },
    Status {
        label: String,
    },
    Deps {
        dep_ids: Vec<String>,
    },
    Person {
        monday_user_ids: Vec<String>,
    },
    Workload {
        value: f32,
    },
    Name {
        name: String,
    },
}

/// Intermediate result of the diff-compute phases (1, 1b, 2).
struct DiffResult {
    ops: Vec<PushOp>,
    working_map: Vec<ItemNodeMapping>,
    skipped: usize,
    new_item_count: usize,
    timeline_col: String,
    status_col: String,
    dep_col: String,
    person_col: String,
    workload_col: String,
}

/// Fetch the current Monday board state, create items for unmapped plinko nodes,
/// and compute the set of field-level update operations — without executing them.
fn compute_push_diff(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
    on_status: impl Fn(&str),
) -> Result<DiffResult, MondayApiError> {
    let timeline_col = config.column_map.timeline_column_id.clone();
    let status_col = config.column_map.status_column_id.clone();
    let dep_col = config.column_map.dependency_column_id.clone();
    let person_col = config.column_map.person_column_id.clone();
    let workload_col = config.column_map.workload_column_id.clone();

    if timeline_col.is_empty() {
        return Err(MondayApiError(
            "Timeline column ID is not configured.".to_string(),
        ));
    }

    // ── Phase 1: fetch current Monday state ───────────────────────────────────
    on_status("Fetching current Monday board state...");
    let monday_items = client.fetch_items(
        &config.board_id,
        &person_col,
        &status_col,
        &dep_col,
        &workload_col,
        &timeline_col,
    )?;

    // When using subitems, the effective board ID for mutations is different from
    // the main board. Fetch it once here so we can use it as a fallback for map
    // entries whose board_id field is empty (saved before this field was introduced).
    let subitems_board_id: Option<String> = if config.use_subitems {
        client
            .fetch_subitem_board_id(&config.board_id)
            .ok()
            .filter(|s| !s.is_empty())
    } else {
        None
    };

    let monday_map: HashMap<&str, &plinko_shared::monday::MondayItem> =
        monday_items.iter().map(|i| (i.id.as_str(), i)).collect();

    // Build a frequency map of monday_user_id → number of times seen across
    // all fetched items. Used to pick the "most-used" account when multiple
    // Monday accounts map to the same plinko user.
    let mut monday_user_freq: HashMap<&str, usize> = HashMap::new();
    for item in &monday_items {
        for uid in &item.assigned_user_ids {
            *monday_user_freq.entry(uid.as_str()).or_insert(0) += 1;
        }
    }

    // For a given plinko UserId, resolve the best Monday user ID: if multiple
    // Monday accounts map to the same plinko user, pick the most frequently seen.
    let resolve_monday_users = |node_id: &NodeId| -> Vec<String> {
        let task_id = match node_id {
            NodeId::Task(id) => id,
            _ => return Vec::new(),
        };
        let Some(task) = plan.tasks.get(task_id) else {
            return Vec::new();
        };
        let plinko_user_ids: Vec<_> = task.assigned_users().collect();
        let mut result = Vec::new();
        for plinko_uid in plinko_user_ids {
            // All Monday accounts mapping to this plinko user.
            let candidates: Vec<&str> = config
                .user_mappings
                .iter()
                .filter(|m| m.plinko_user_id == Some(plinko_uid))
                .map(|m| m.monday_user_id.as_str())
                .collect();
            if candidates.is_empty() {
                continue;
            }
            // Pick the most-frequently-seen account; fall back to first.
            let best = candidates
                .iter()
                .max_by_key(|id| monday_user_freq.get(*id).copied().unwrap_or(0))
                .copied()
                .unwrap_or(candidates[0]);
            result.push(best.to_string());
        }
        result
    };

    // Total workload for a task in the configured unit (hours or days).
    let task_workload = |task_id: &plinko_shared::data::ids::TaskId| -> Option<f32> {
        let task = plan.tasks.get(task_id)?;
        let days: f32 = task.workers.iter().map(|w| w.workload_days()).sum();
        Some(if config.workload_in_hours {
            days * 8.0
        } else {
            days
        })
    };

    // ── Phase 1b: create Monday items for unmapped plinko nodes ───────────────
    // Build a mutable copy of the map so we can extend it with new entries.
    let mut working_map: Vec<ItemNodeMapping> = item_node_map.to_vec();
    let mut new_item_count = 0usize;

    // Collect all plinko node IDs that have no Monday mapping yet.
    let mapped_nodes: std::collections::HashSet<NodeId> =
        working_map.iter().map(|m| m.plinko_node_id).collect();

    let unmapped_tasks: Vec<_> = plan
        .tasks
        .keys()
        .filter(|id| !mapped_nodes.contains(&NodeId::Task(**id)))
        .collect();
    let unmapped_milestones: Vec<_> = plan
        .milestones
        .keys()
        .filter(|id| !mapped_nodes.contains(&NodeId::Milestone(**id)))
        .collect();

    if !unmapped_tasks.is_empty() || !unmapped_milestones.is_empty() {
        on_status(&format!(
            "Creating {} new item(s) on Monday...",
            unmapped_tasks.len() + unmapped_milestones.len()
        ));
        let groups = client.fetch_groups(&config.board_id)?;
        let group_id =
            if let Some((id, _)) = groups.iter().find(|(_, title)| title == FROM_PLINKO_GROUP) {
                id.clone()
            } else {
                client.create_group(&config.board_id, FROM_PLINKO_GROUP)?
            };

        // In subitems mode we create tasks as subitems of a single "From Plinko"
        // parent item; milestones are always created as top-level items.
        let parent_item_id: Option<String> = if config.use_subitems && !unmapped_tasks.is_empty() {
            // Find existing "From Plinko" top-level item, or create one.
            let existing_parent = monday_items
                .iter()
                .find(|i| i.parent_id.is_none() && i.name == FROM_PLINKO_PARENT);
            if let Some(p) = existing_parent {
                Some(p.id.clone())
            } else {
                let pid = client.create_item(&config.board_id, &group_id, FROM_PLINKO_PARENT)?;
                Some(pid)
            }
        } else {
            None
        };

        // Create unmapped tasks.
        for task_id in &unmapped_tasks {
            let name = plan
                .tasks
                .get(task_id)
                .map(|t| t.name.as_str())
                .unwrap_or("Unnamed Task");

            let (monday_id, board_id) = if let Some(ref pid) = parent_item_id {
                // Subitems mode.
                let (sub_id, sub_board) = client.create_subitem(pid, name)?;
                (sub_id, sub_board)
            } else {
                let item_id = client.create_item(&config.board_id, &group_id, name)?;
                (item_id, config.board_id.clone())
            };

            working_map.push(ItemNodeMapping {
                monday_item_id: monday_id,
                plinko_node_id: NodeId::Task(**task_id),
                board_id,
            });
            new_item_count += 1;
        }

        // Create unmapped milestones (always as top-level items).
        for ms_id in &unmapped_milestones {
            let name = plan
                .milestones
                .get(ms_id)
                .map(|m| m.name.as_str())
                .unwrap_or("Unnamed Milestone");
            let monday_id = client.create_item(&config.board_id, &group_id, name)?;
            working_map.push(ItemNodeMapping {
                monday_item_id: monday_id,
                plinko_node_id: NodeId::Milestone(**ms_id),
                board_id: config.board_id.clone(),
            });
            new_item_count += 1;
        }
    }

    // Rebuild lookup closures against the now-complete working_map.
    let find_monday_id = |node: &NodeId| -> Option<&str> {
        working_map
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
    on_status("Computing changes...");
    let mut ops: Vec<PushOp> = Vec::new();
    let mut skipped = 0usize;

    for mapping in &working_map {
        let monday_item_id = &mapping.monday_item_id;
        let board_id = if !mapping.board_id.is_empty() {
            mapping.board_id.as_str()
        } else if let Some(ref sub_bid) = subitems_board_id {
            sub_bid.as_str()
        } else {
            config.board_id.as_str()
        };
        let current = monday_map.get(monday_item_id.as_str());

        // Resolve a human-readable name for debug messages.
        let node_name = || match &mapping.plinko_node_id {
            NodeId::Task(id) => plan
                .tasks
                .get(id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| format!("task:{}", id.0)),
            NodeId::Milestone(id) => plan
                .milestones
                .get(id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| format!("milestone:{}", id.0)),
            NodeId::PlanStart => "PlanStart".to_string(),
        };

        // Timeline
        let timeline = match &mapping.plinko_node_id {
            NodeId::Task(task_id) => {
                let Some(state) = plan.node_allocations.tasks.get(task_id) else {
                    eprintln!(
                        "Warning: skipping '{}' (monday id {}) — no scheduler allocation (task not yet scheduled)",
                        node_name(),
                        monday_item_id
                    );
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
                    false,
                ))
            }
            NodeId::Milestone(ms_id) => {
                let Some(ms_alloc) = plan.node_allocations.milestones.get(ms_id) else {
                    eprintln!(
                        "Warning: skipping '{}' (monday id {}) — no scheduler allocation (milestone not yet scheduled)",
                        node_name(),
                        monday_item_id
                    );
                    skipped += 1;
                    continue;
                };
                let d = ms_alloc.date().format("%Y-%m-%d").to_string();
                Some((d.clone(), d, true))
            }
            NodeId::PlanStart => {
                skipped += 1;
                continue;
            }
        };

        if let Some((from, to, is_milestone)) = timeline {
            let needs_update = current.is_none_or(|item| {
                let cur_from = item
                    .timeline_start
                    .map(|d| d.format("%Y-%m-%d").to_string());
                let cur_to = item.timeline_end.map(|d| d.format("%Y-%m-%d").to_string());
                let cur_is_milestone = item.is_milestone;
                cur_from.as_deref() != Some(&from)
                    || cur_to.as_deref() != Some(&to)
                    || cur_is_milestone != is_milestone
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.to_owned(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Timeline {
                        from,
                        to,
                        is_milestone,
                    },
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
                    current.is_none_or(|item| item.status_label.as_deref() != Some(label));
                if needs_update {
                    ops.push(PushOp {
                        board_id: board_id.to_owned(),
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

            let needs_update = current.is_none_or(|item| {
                let mut cur_deps = item.dependency_item_ids.clone();
                cur_deps.sort();
                cur_deps != plinko_dep_ids
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.to_owned(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Deps {
                        dep_ids: plinko_dep_ids,
                    },
                });
            }
        }

        // Person column (tasks only)
        if !person_col.is_empty()
            && let NodeId::Task(_) = &mapping.plinko_node_id
        {
            let plinko_user_ids = resolve_monday_users(&mapping.plinko_node_id);
            let mut sorted_plinko = plinko_user_ids.clone();
            sorted_plinko.sort();
            let needs_update = current.is_none_or(|item| {
                let mut cur = item.assigned_user_ids.clone();
                cur.sort();
                cur != sorted_plinko
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.to_owned(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Person {
                        monday_user_ids: plinko_user_ids,
                    },
                });
            }
        }

        // Name diff
        let plinko_name = match &mapping.plinko_node_id {
            NodeId::Task(task_id) => plan.tasks.get(task_id).map(|t| t.name.as_str()),
            NodeId::Milestone(ms_id) => plan.milestones.get(ms_id).map(|m| m.name.as_str()),
            NodeId::PlanStart => None,
        };
        if let Some(name) = plinko_name {
            let needs_update = current.is_none_or(|item| item.name.trim() != name.trim());
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.to_owned(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Name {
                        name: name.to_string(),
                    },
                });
            }
        }

        // Workload column (tasks only)
        if !workload_col.is_empty()
            && let NodeId::Task(task_id) = &mapping.plinko_node_id
            && let Some(plinko_wl) = task_workload(task_id)
        {
            let needs_update = current.is_none_or(|item| {
                item.workload
                    .is_none_or(|cur_wl| (cur_wl - plinko_wl).abs() > 0.01)
            });
            if needs_update {
                ops.push(PushOp {
                    board_id: board_id.to_owned(),
                    item_id: monday_item_id.clone(),
                    kind: PushKind::Workload { value: plinko_wl },
                });
            }
        }
    }

    Ok(DiffResult {
        ops,
        working_map,
        skipped,
        new_item_count,
        timeline_col,
        status_col,
        dep_col,
        person_col,
        workload_col,
    })
}

/// Preview how many updates a push would generate, without executing anything.
///
/// Returns `(op_count, new_item_count)`.
pub fn preview_push_counts(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
) -> Result<(usize, usize), MondayApiError> {
    let result = compute_push_diff(client, config, plan, item_node_map, |msg| {
        eprintln!("[preview] {msg}");
    })?;
    Ok((result.ops.len(), result.new_item_count))
}

/// Diff-based export: fetch current Monday state, compute what actually changed,
/// then push only those updates. Also creates items on Monday for any plinko
/// tasks/milestones that have no existing Monday mapping.
///
/// `on_progress` is called with `(done, total, message)` as work proceeds.
///
/// Returns the human-readable status message and the fully-updated item→node map
/// (existing + newly-created entries). Callers should persist this map.
pub fn export_to_monday_diff(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
    on_progress: impl Fn(usize, usize, &str),
) -> Result<(String, Vec<ItemNodeMapping>), MondayApiError> {
    let DiffResult {
        ops,
        working_map,
        skipped,
        new_item_count: created_count,
        timeline_col,
        status_col,
        dep_col,
        person_col,
        workload_col,
    } = compute_push_diff(client, config, plan, item_node_map, |msg| {
        eprintln!("[push] {msg}");
        on_progress(0, 0, msg);
    })?;

    // ── Phase 3: execute ops with progress tracking ───────────────────────────
    let total = ops.len();
    eprintln!("[push] Phase 3: {total} ops to execute ({skipped} items skipped)");
    on_progress(0, total, "Starting push...");

    if total == 0 {
        let msg = if created_count > 0 {
            format!(
                "Created {created_count} new item(s); nothing else to update ({skipped} skipped)."
            )
        } else {
            format!("Nothing to update ({skipped} items skipped — already up to date).")
        };
        eprintln!("[push] {msg}");
        on_progress(0, 0, &msg);
        return Ok((msg, working_map));
    }

    let status_msg = format!("Pushing {total} update(s) to Monday...");
    eprintln!("[push] {status_msg}");
    on_progress(0, 0, &status_msg);

    let mut updated = 0usize;
    let mut failed = 0usize;

    for (i, op) in ops.into_iter().enumerate() {
        let kind_label = match &op.kind {
            PushKind::Timeline { .. } => "timeline",
            PushKind::Status { .. } => "status",
            PushKind::Deps { .. } => "deps",
            PushKind::Person { .. } => "person",
            PushKind::Workload { .. } => "workload",
            PushKind::Name { .. } => "name",
        };
        eprintln!(
            "[push] [{kind_label}] item {} ({}/{})",
            op.item_id,
            i + 1,
            total
        );
        let result = match op.kind {
            PushKind::Timeline {
                from,
                to,
                is_milestone,
            } => client.update_timeline(
                &op.board_id,
                &op.item_id,
                &timeline_col,
                &from,
                &to,
                is_milestone,
            ),
            PushKind::Status { label } => {
                client.update_status(&op.board_id, &op.item_id, &status_col, &label)
            }
            PushKind::Deps { dep_ids } => {
                let dep_refs: Vec<&str> = dep_ids.iter().map(|s| s.as_str()).collect();
                client.update_dependencies(&op.board_id, &op.item_id, &dep_col, &dep_refs)
            }
            PushKind::Person { monday_user_ids } => {
                let refs: Vec<&str> = monday_user_ids.iter().map(|s| s.as_str()).collect();
                client.update_person(&op.board_id, &op.item_id, &person_col, &refs)
            }
            PushKind::Workload { value } => {
                client.update_workload(&op.board_id, &op.item_id, &workload_col, value)
            }
            PushKind::Name { name } => client.rename_item(&op.board_id, &op.item_id, &name),
        };
        match result {
            Ok(()) => updated += 1,
            Err(e) => {
                eprintln!("Warning: push op failed for {}: {e}", op.item_id);
                failed += 1;
            }
        }
        on_progress(i + 1, total, "Pushing updates...");
    }

    let msg = if failed == 0 {
        if created_count > 0 {
            format!("Push complete: {created_count} created, {updated} updated, {skipped} skipped.")
        } else {
            format!("Push complete: {updated} updated, {skipped} skipped.")
        }
    } else if created_count > 0 {
        format!(
            "Push complete: {created_count} created, {updated} updated, {failed} failed, {skipped} skipped."
        )
    } else {
        format!("Push complete: {updated} updated, {failed} failed, {skipped} skipped.")
    };
    Ok((msg, working_map))
}
