import { useState } from "react";
import { Modal } from "../Modal";
import type { DateConstraint, Dependency, Milestone, NodeId, Plan, PlanRequest, PlanResponse, Status, Task, TaskId, TaskPatch, WorkerSlot } from "../../protocol";
import { DependencyEditor } from "./shared/DependencyEditor";
import { WorkerEditor } from "./shared/WorkerEditor";
import { ConstraintEditor } from "./shared/ConstraintEditor";
import { SegmentedControl } from "./shared/SegmentedControl";
import { DatePicker } from "./shared/DatePicker";
import { STATUS_LABELS } from "../../utils/planUtils";
import { v4 as uuidv4 } from "uuid";

function nodeKey(n: NodeId): string {
  if (n === "PlanStart") return "PlanStart";
  if (typeof n === "object" && "Task" in n) return `task:${n.Task}`;
  if (typeof n === "object" && "Milestone" in n) return `milestone:${n.Milestone}`;
  return String(n);
}

interface Props {
  task: Task | null;
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

const STATUSES: Status[] = ["NotStarted", "InProgress", "OnHold", "Complete", "Dropped"];

/** Build initial forward-dependents list: nodes that currently depend on this task */
function buildInitialDependents(task: Task | null, plan: Plan): Dependency[] {
  if (!task) return [];
  const result: Dependency[] = [];
  for (const t of Object.values(plan.tasks)) {
    for (const d of t.dependencies) {
      if (typeof d.id === "object" && "Task" in d.id && d.id.Task === task.id) {
        result.push({ id: { Task: t.id as TaskId }, lag_days: d.lag_days });
      }
    }
  }
  for (const m of Object.values(plan.milestones) as Milestone[]) {
    for (const d of m.dependencies) {
      if (typeof d.id === "object" && "Task" in d.id && d.id.Task === task.id) {
        result.push({ id: { Milestone: m.id }, lag_days: d.lag_days });
      }
    }
  }
  return result;
}

export function TaskFormModal({ task, plan, sendRequest, onClose }: Props) {
  const [name, setName] = useState(task?.name ?? "");
  const [description, setDescription] = useState(task?.description ?? "");
  const [status, setStatus] = useState<Status>(
    task ? (plan.node_allocations.tasks[task.id]?.status ?? "NotStarted") : "NotStarted"
  );
  const [durationDays, setDurationDays] = useState(
    String(task?.duration_days_target ?? 0)
  );
  const [constraint, setConstraint] = useState<DateConstraint | null>(task?.constraint ?? null);
  const [actualStart, setActualStart] = useState(task?.actual_start ?? "");
  const [workers, setWorkers] = useState<WorkerSlot[]>(task?.workers ?? []);
  const [dependencies, setDependencies] = useState<Dependency[]>(task?.dependencies ?? []);
  const [dependents, setDependents] = useState<Dependency[]>(() => buildInitialDependents(task, plan));
  const [relaxed, setRelaxed] = useState(task?.relaxed_mode ?? false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const thisNodeId: NodeId | null = task ? { Task: task.id } : null;

  // Keys of nodes already in the dependents list, to exclude from the picker
  const dependentKeys = new Set(dependents.map((d) => {
    if (typeof d.id === "object" && "Task" in d.id) return `task:${d.id.Task}`;
    if (typeof d.id === "object" && "Milestone" in d.id) return `milestone:${d.id.Milestone}`;
    return String(d.id);
  }));

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const dur = parseFloat(durationDays) || 0;
      const actualStartVal = actualStart || null;
      const newTaskId = task?.id ?? uuidv4();
      const newNodeId: NodeId = { Task: newTaskId as TaskId };

      if (task) {
        const patch: TaskPatch = {
          name: name.trim(),
          description,
          status,
          actual_start_date: actualStartVal,
          constraint,
          duration_days_target: dur,
          workers,
          dependencies,
          relaxed_mode: relaxed,
        };
        const resp = await sendRequest({ UpdateTask: [task.id, patch] });
        if (typeof resp === "object" && "Error" in resp) {
          setError(JSON.stringify(resp.Error));
          return;
        }
      } else {
        const resp = await sendRequest({
          CreateTask: {
            id: newTaskId,
            name: name.trim(),
            description,
            constraint,
            duration_days_target: dur,
            workers,
            dependencies,
            relaxed_mode: relaxed,
            actual_start: actualStartVal,
            context_label: null,
          },
        });
        if (typeof resp === "object" && "Error" in resp) {
          setError(JSON.stringify(resp.Error));
          return;
        }
      }

      // Sync forward dependents: diff old vs new
      const oldDependents = buildInitialDependents(task, plan);
      const oldKeys = new Set(oldDependents.map((d) => nodeKey(d.id)));
      const newDepMap = new Map(dependents.map((d) => [nodeKey(d.id), d]));
      const newKeys = new Set(newDepMap.keys());

      // Removed: strip our node from their dependencies
      for (const old of oldDependents) {
        const k = nodeKey(old.id);
        if (!newKeys.has(k)) {
          if (typeof old.id === "object" && "Task" in old.id) {
            const t = plan.tasks[old.id.Task];
            if (t) {
              await sendRequest({ UpdateTask: [t.id, {
                dependencies: t.dependencies.filter((d) => nodeKey(d.id) !== nodeKey(newNodeId)),
              }] });
            }
          } else if (typeof old.id === "object" && "Milestone" in old.id) {
            const m = plan.milestones[old.id.Milestone];
            if (m) {
              await sendRequest({ UpdateMilestone: [m.id, {
                dependencies: m.dependencies.filter((d) => nodeKey(d.id) !== nodeKey(newNodeId)),
              }] });
            }
          }
        }
      }
      // Added: add our node to their dependencies
      for (const dep of dependents) {
        const k = nodeKey(dep.id);
        if (!oldKeys.has(k)) {
          if (typeof dep.id === "object" && "Task" in dep.id) {
            const t = plan.tasks[dep.id.Task];
            if (t) {
              await sendRequest({ UpdateTask: [t.id, {
                dependencies: [...t.dependencies, { id: newNodeId, lag_days: dep.lag_days }],
              }] });
            }
          } else if (typeof dep.id === "object" && "Milestone" in dep.id) {
            const m = plan.milestones[dep.id.Milestone];
            if (m) {
              await sendRequest({ UpdateMilestone: [m.id, {
                dependencies: [...m.dependencies, { id: newNodeId, lag_days: dep.lag_days }],
              }] });
            }
          }
        }
      }

      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!task) return;
    setSaving(true);
    try {
      await sendRequest({ DeleteTask: task.id });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title={task ? task.name : "New Task"} onClose={onClose} width={500}>
      {error && (
        <div style={{ color: "#e57373", fontSize: 13, marginBottom: 12 }}>{error}</div>
      )}
      <div className="form-row">
        <label>Name</label>
        <input
          type="text"
          value={name}
          autoFocus={!task}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      <div className="form-row">
        <label>Description</label>
        <textarea
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          rows={4}
        />
      </div>

      {/* Status */}
      <div className="form-row">
        <label>Status</label>
        <SegmentedControl
          options={STATUSES.map((s) => STATUS_LABELS[s])}
          selected={STATUSES.indexOf(status)}
          onChange={(i) => setStatus(STATUSES[i])}
        />
      </div>

      {/* Duration + Mode (two columns) */}
      <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
          <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
            Duration (days)
          </label>
          <input
            type="number"
            min={0}
            step={0.5}
            value={durationDays}
            onChange={(e) => setDurationDays(e.target.value)}
            style={{
              background: "#1e1e1e",
              border: "1px solid #3a3a3c",
              borderRadius: 4,
              color: "#d4d4d4",
              fontSize: 13,
              padding: "0 10px",
              outline: "none",
              width: "100%",
              boxSizing: "border-box",
              height: 30,
            }}
          />
        </div>
        <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
          <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
            Mode
          </label>
          <button
            onClick={() => setRelaxed(!relaxed)}
            style={{
              background: relaxed ? "#3a3a3c" : "#4a90d9",
              border: "1px solid #3a3a3c",
              borderRadius: 4,
              color: relaxed ? "#d4d4d4" : "#fff",
              fontSize: 13,
              fontWeight: relaxed ? 400 : 600,
              cursor: "pointer",
              fontFamily: "inherit",
              height: 30,
              width: "100%",
            }}
          >
            {relaxed ? "Relaxed" : "Strict"}
          </button>
        </div>
      </div>

      <ConstraintEditor value={constraint} onChange={setConstraint} />

      {/* Actual dates (two columns) */}
      {(() => {
        const startDisabled = status === "NotStarted";
        const endDisabled = status !== "Complete" && status !== "Dropped";
        return (
          <div style={{ display: "flex", gap: 12, marginBottom: 16 }}>
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
              <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
                Actual Start
              </label>
              <DatePicker
                value={actualStart}
                onChange={setActualStart}
                disabled={startDisabled}
              />
            </div>
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
              <label style={{ fontSize: 12, color: "#999", textTransform: "uppercase", letterSpacing: "0.04em" }}>
                Actual End
              </label>
              <DatePicker
                value={""}
                onChange={() => {}}
                disabled={endDisabled}
              />
            </div>
          </div>
        );
      })()}

      {/* Workers */}
      <WorkerEditor workers={workers} plan={plan} onChange={setWorkers} />

      <DependencyEditor
        label="Dependencies"
        deps={dependencies}
        plan={plan}
        excludeNodeId={thisNodeId}
        onChange={setDependencies}
      />

      {/* Forward dependents — editable, like Rust UI ("Required by") */}
      <DependencyEditor
        label="Required by"
        deps={dependents}
        plan={plan}
        excludeNodeId={thisNodeId}
        excludeKeys={dependentKeys}
        noPlanStart
        onChange={setDependents}
        emptyLabel="Select dependent…"
        emptyStateText="No dependents added yet"
      />

      <div className="form-actions">
        {task && (
          <button className="btn btn-danger" onClick={handleDelete} disabled={saving}>
            Delete
          </button>
        )}
        <button className="btn btn-secondary" onClick={onClose}>
          Cancel
        </button>
        <button
          className="btn btn-primary"
          onClick={handleSave}
          disabled={saving || !name.trim()}
        >
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}
