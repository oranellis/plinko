//! Plinko → Monday.com export logic.
//!
//! Writes computed start and end dates back to Monday.com timeline columns.

use plinko_shared::data::Plan;
use plinko_shared::data::allocation::TaskAllocation;
use plinko_shared::data::ids::NodeId;
use plinko_shared::monday::{ItemNodeMapping, MondayConfig};

use crate::monday::client::{MondayApiError, MondayClient};

/// Export scheduled dates to Monday.com timeline columns.
///
/// For each item in `item_node_map`, reads the scheduled start/end date from
/// the plan allocations and updates the Monday timeline column.
/// Returns a status message.
pub fn export_to_monday(
    client: &MondayClient,
    config: &MondayConfig,
    plan: &Plan,
    item_node_map: &[ItemNodeMapping],
) -> Result<String, MondayApiError> {
    let timeline_col = &config.column_map.timeline_column_id;
    if timeline_col.is_empty() {
        return Err(MondayApiError(
            "Timeline column ID is not configured.".to_string(),
        ));
    }

    let mut updated = 0usize;
    let mut skipped = 0usize;

    for mapping in item_node_map {
        let (from, to) = match &mapping.plinko_node_id {
            NodeId::Task(task_id) => {
                let Some(state) = plan.node_allocations.tasks.get(task_id) else {
                    skipped += 1;
                    continue;
                };
                let start = state.allocation.start_date();
                let end = state.allocation.end_date();
                // Use corrected end date if available (for Fixed allocations).
                let end = match &state.allocation {
                    TaskAllocation::Fixed {
                        corrected_end_date: Some(c),
                        ..
                    } => *c,
                    _ => end,
                };
                (
                    start.format("%Y-%m-%d").to_string(),
                    end.format("%Y-%m-%d").to_string(),
                )
            }
            NodeId::Milestone(ms_id) => {
                let Some(ms_alloc) = plan.node_allocations.milestones.get(ms_id) else {
                    skipped += 1;
                    continue;
                };
                let date: chrono::NaiveDate = ms_alloc.date();
                let d = date.format("%Y-%m-%d").to_string();
                (d.clone(), d)
            }
            NodeId::PlanStart => {
                skipped += 1;
                continue;
            }
        };

        match client.update_timeline(
            &config.board_id,
            &mapping.monday_item_id,
            timeline_col,
            &from,
            &to,
        ) {
            Ok(()) => updated += 1,
            Err(e) => {
                // Log but continue with other items.
                eprintln!(
                    "Warning: failed to update item {}: {e}",
                    mapping.monday_item_id
                );
                skipped += 1;
            }
        }
    }

    Ok(format!(
        "Export complete: {updated} updated, {skipped} skipped."
    ))
}
