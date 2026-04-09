/**
 * Shared utility helpers for the Plinko React UI.
 */

import {
  IsoDate,
  MilestoneId,
  NodeAllocations,
  NodeId,
  Plan,
  Status,
  Task,
  TaskId,
  WorkerSlot,
} from "../protocol";

// ── Date helpers ─────────────────────────────────────────────────────────────

export function parseDate(s: IsoDate): Date {
  const [y, m, d] = s.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function formatDate(d: Date): IsoDate {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

export function todayIso(): IsoDate {
  return formatDate(new Date());
}

/** Days between two ISO date strings (end - start). */
export function daysBetween(a: IsoDate, b: IsoDate): number {
  return Math.round(
    (parseDate(b).getTime() - parseDate(a).getTime()) / 86_400_000
  );
}

/** Add n days to an ISO date string. */
export function addDays(iso: IsoDate, n: number): IsoDate {
  const d = parseDate(iso);
  d.setDate(d.getDate() + n);
  return formatDate(d);
}

// ── NodeId helpers ────────────────────────────────────────────────────────────

export function nodeIdString(id: NodeId): string {
  if (id === "PlanStart") return "plan_start";
  if ("Task" in id) return `task:${id.Task}`;
  return `milestone:${id.Milestone}`;
}

export function taskNodeId(id: TaskId): NodeId {
  return { Task: id };
}

export function milestoneNodeId(id: MilestoneId): NodeId {
  return { Milestone: id };
}

// ── Allocation helpers ────────────────────────────────────────────────────────

export interface NodeDates {
  start: IsoDate;
  end: IsoDate;
}

export function taskDates(
  taskId: TaskId,
  allocs: NodeAllocations
): NodeDates | null {
  const state = allocs.tasks[taskId];
  if (!state) return null;
  const a = state.allocation;
  if ("Fixed" in a) {
    return { start: a.Fixed.start_date, end: a.Fixed.corrected_end_date ?? a.Fixed.end_date };
  }
  if ("Dynamic" in a) {
    return { start: a.Dynamic.scheduled_start_date, end: a.Dynamic.scheduled_end_date };
  }
  return null;
}

export function milestoneDates(
  msId: MilestoneId,
  allocs: NodeAllocations
): NodeDates | null {
  const ms = allocs.milestones[msId];
  if (!ms) return null;
  return { start: ms.date, end: ms.date };
}

// ── Status helpers ────────────────────────────────────────────────────────────

export const STATUS_COLORS: Record<Status, string> = {
  NotStarted: "#4a90d9",
  InProgress: "#4caf50",
  OnHold: "#ff9800",
  Complete: "#26a69a",
  Dropped: "#666666",
};

export const STATUS_LABELS: Record<Status, string> = {
  NotStarted: "Not Started",
  InProgress: "In Progress",
  OnHold: "On Hold",
  Complete: "Complete",
  Dropped: "Dropped",
};

export function taskStatus(taskId: TaskId, allocs: NodeAllocations): Status {
  return allocs.tasks[taskId]?.status ?? "NotStarted";
}

// ── Worker helpers ────────────────────────────────────────────────────────────

export function workerUserId(slot: WorkerSlot): string | null {
  if ("Specific" in slot) return slot.Specific.user_id;
  return null;
}

export function workerWorkload(slot: WorkerSlot): number {
  if ("Specific" in slot) return slot.Specific.workload_days;
  if ("Placeholder" in slot) return slot.Placeholder.workload_days;
  return 0;
}

// ── Display name ─────────────────────────────────────────────────────────────

export function displayName(name: string, contextLabel?: string | null): string {
  if (contextLabel) return `${name} | ${contextLabel}`;
  return name;
}

// ── Plan summary helpers ──────────────────────────────────────────────────────

export function getUserName(plan: Plan, userId: string): string {
  return plan.users_data[userId]?.user.name ?? userId.slice(0, 8);
}

export function getTasksForUser(plan: Plan, userId: string): Task[] {
  return Object.values(plan.tasks).filter((t) =>
    t.workers.some((w) => workerUserId(w) === userId)
  );
}

// ── Gantt packing ────────────────────────────────────────────────────────────

export interface GanttItem {
  id: string;
  type: "task" | "milestone";
  name: string;
  contextLabel: string | null;
  start: IsoDate;
  end: IsoDate;
  status: Status;
  row: number;
}

/** Pack tasks and milestones into rows (greedy bin packing). */
export function packGanttRows(plan: Plan): GanttItem[] {
  const items: Omit<GanttItem, "row">[] = [];

  for (const [id, task] of Object.entries(plan.tasks)) {
    const dates = taskDates(id as TaskId, plan.node_allocations);
    if (!dates) continue;
    items.push({
      id,
      type: "task",
      name: task.name,
      contextLabel: task.context_label ?? null,
      start: dates.start,
      end: dates.end,
      status: taskStatus(id as TaskId, plan.node_allocations),
    });
  }

  for (const [id, ms] of Object.entries(plan.milestones)) {
    const dates = milestoneDates(id as MilestoneId, plan.node_allocations);
    if (!dates) continue;
    items.push({
      id,
      type: "milestone",
      name: ms.name,
      contextLabel: ms.context_label ?? null,
      start: dates.start,
      end: dates.end,
      status: "NotStarted",
    });
  }

  // Sort by start date, then name.
  items.sort((a, b) => a.start.localeCompare(b.start) || a.name.localeCompare(b.name));

  // Greedy row packing: track latest end date per row.
  const rowEnds: IsoDate[] = [];
  const result: GanttItem[] = [];

  for (const item of items) {
    let placed = false;
    for (let r = 0; r < rowEnds.length; r++) {
      if (item.start > rowEnds[r]) {
        rowEnds[r] = item.end;
        result.push({ ...item, row: r });
        placed = true;
        break;
      }
    }
    if (!placed) {
      const r = rowEnds.length;
      rowEnds.push(item.end);
      result.push({ ...item, row: r });
    }
  }

  return result;
}
