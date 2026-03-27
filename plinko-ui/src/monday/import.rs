//! Monday.com → plinko import logic.
//!
//! Converts Monday board items into plinko [`PlanRequest`]s sent via the
//! [`PlanRequestSender`]. Uses the [`MondayConfig`] mappings to resolve
//! users, statuses, and dependencies.

use std::collections::HashMap;

use plinko_shared::data::Dependency;
use plinko_shared::data::allocation::Status;
use plinko_shared::data::ids::{MilestoneId, NodeId, TaskId};
use plinko_shared::data::milestone::Milestone;
use plinko_shared::data::task::{Task, WorkerSlot};
use plinko_shared::monday::{ItemNodeMapping, MondayConfig, MondayItem};
use plinko_shared::protocol::{PlanRequest, TaskPatch};

use crate::engine::PlanRequestSender;
use crate::monday::client::{MondayApiError, MondayClient};

/// Import Monday board items into the plan.
///
/// Returns an updated `MondayConfig` (with new `item_node_map` entries) and a
/// status message. Sends `PlanRequest`s directly via `sender`.
pub fn import_from_monday(
    client: &MondayClient,
    config: &MondayConfig,
    sender: &PlanRequestSender,
) -> Result<(Vec<ItemNodeMapping>, String), MondayApiError> {
    let cm = &config.column_map;

    let items = client.fetch_items(
        &config.board_id,
        &cm.person_column_id,
        &cm.status_column_id,
        &cm.dependency_column_id,
        &cm.workload_column_id,
        config.use_subitems,
    )?;

    // Build a lookup from Monday item ID → existing plinko node ID.
    let existing: HashMap<String, NodeId> = config
        .item_node_map
        .iter()
        .map(|m| (m.monday_item_id.clone(), m.plinko_node_id.clone()))
        .collect();

    // Build a lookup from Monday item ID → plinko node ID (mutable — we add new entries).
    let mut id_map: HashMap<String, NodeId> = existing.clone();

    let mut created = 0usize;
    let mut updated = 0usize;

    // --- Pass 1: create/update all nodes ---
    for item in &items {
        let is_milestone = config.use_subitems && item.parent_id.is_none();

        if let Some(node_id) = existing.get(&item.id) {
            // Update existing node.
            match node_id {
                NodeId::Task(task_id) => {
                    let patch = build_task_patch(item, config);
                    sender.send(PlanRequest::UpdateTask(*task_id, patch));
                    let status = resolve_status(item, config);
                    apply_status_to_existing_task(*task_id, status, sender);
                }
                NodeId::Milestone(_ms_id) => {
                    // Nothing to update for milestones beyond what's in MilestonePatch.
                }
                NodeId::PlanStart => {}
            }
            updated += 1;
        } else {
            // Create a new node.
            let node_id = if is_milestone {
                let ms = Milestone::new(&item.name, "");
                let ms_id = ms.id;
                sender.send(PlanRequest::CreateMilestone(ms));
                NodeId::Milestone(ms_id)
            } else {
                let task = build_task(item, config);
                let task_id = task.id;
                let status = resolve_status(item, config);
                sender.send(PlanRequest::CreateTask(task));
                apply_status_to_existing_task(task_id, status, sender);
                NodeId::Task(task_id)
            };
            id_map.insert(item.id.clone(), node_id);
            created += 1;
        }
    }

    // --- Pass 2: wire dependencies ---
    for item in &items {
        if item.dependency_item_ids.is_empty() {
            continue;
        }
        let Some(this_node) = id_map.get(&item.id) else {
            continue;
        };

        let deps: Vec<Dependency> = item
            .dependency_item_ids
            .iter()
            .filter_map(|dep_monday_id| {
                let dep_node = id_map.get(dep_monday_id)?;
                Some(Dependency::new(dep_node.clone()))
            })
            .collect();

        if deps.is_empty() {
            continue;
        }

        match this_node.clone() {
            NodeId::Task(task_id) => {
                sender.send(PlanRequest::UpdateTask(
                    task_id,
                    TaskPatch::new().dependencies(deps),
                ));
            }
            NodeId::Milestone(ms_id) => {
                sender.send(PlanRequest::UpdateMilestone(
                    ms_id,
                    plinko_shared::protocol::MilestonePatch::new().dependencies(deps),
                ));
            }
            NodeId::PlanStart => {}
        }
    }

    // --- Pass 3: run scheduler ---
    sender.send(PlanRequest::RunScheduler);

    // Build updated item_node_map.
    let new_map: Vec<ItemNodeMapping> = id_map
        .into_iter()
        .map(|(monday_item_id, plinko_node_id)| ItemNodeMapping {
            monday_item_id,
            plinko_node_id,
        })
        .collect();

    let message = format!("Import complete: {created} created, {updated} updated.");
    Ok((new_map, message))
}

// ── Helpers ─────────────────────────────────────────────────────────────────── {{{

fn build_task(item: &MondayItem, config: &MondayConfig) -> Task {
    let workers = build_workers(item, config);
    let workload = item.workload.unwrap_or(1.0);
    let workload_days = if config.workload_in_hours {
        workload / 8.0
    } else {
        workload
    };

    let workers = if workers.is_empty() {
        vec![WorkerSlot::Placeholder {
            required_tags: Default::default(),
            workload_days: workload_days.max(0.1),
        }]
    } else {
        workers
            .into_iter()
            .map(|uid| WorkerSlot::Specific {
                user_id: uid,
                workload_days: workload_days.max(0.1),
            })
            .collect()
    };

    Task {
        id: TaskId::new(),
        name: item.name.clone(),
        description: String::new(),
        dependencies: Vec::new(),
        workers,
        constraint: None,
        duration_days_target: 0.0,
        relaxed_mode: false,
        actual_start: None,
    }
}

fn build_task_patch(item: &MondayItem, config: &MondayConfig) -> TaskPatch {
    let workers = build_workers(item, config);
    let workload = item.workload.unwrap_or(1.0);
    let workload_days = if config.workload_in_hours {
        workload / 8.0
    } else {
        workload
    };

    let worker_slots: Vec<WorkerSlot> = if workers.is_empty() {
        vec![WorkerSlot::Placeholder {
            required_tags: Default::default(),
            workload_days: workload_days.max(0.1),
        }]
    } else {
        workers
            .into_iter()
            .map(|uid| WorkerSlot::Specific {
                user_id: uid,
                workload_days: workload_days.max(0.1),
            })
            .collect()
    };

    TaskPatch::new().workers(worker_slots)
}

fn build_workers(
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

fn apply_status_to_existing_task(task_id: TaskId, status: Status, sender: &PlanRequestSender) {
    match status {
        Status::NotStarted => {}
        Status::InProgress => {
            sender.send(PlanRequest::StartTask(task_id));
        }
        Status::OnHold => {
            sender.send(PlanRequest::StartTask(task_id));
            sender.send(PlanRequest::PauseTask(task_id));
        }
        Status::Complete => {
            sender.send(PlanRequest::CompleteTask(task_id));
        }
        Status::Dropped => {
            sender.send(PlanRequest::DropTask(task_id));
        }
    }
}
// }}}
