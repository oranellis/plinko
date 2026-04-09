import { useState } from "react";
import { Modal } from "../Modal";
import type { DateConstraint, Dependency, Plan, PlanRequest, PlanResponse, Status, Task, TaskPatch, UserId, WorkerSlot } from "../../protocol";
import { DependencyEditor } from "./shared/DependencyEditor";
import { ConstraintEditor } from "./shared/ConstraintEditor";
import { STATUS_LABELS, workerUserId, workerWorkload } from "../../utils/planUtils";
import { v4 as uuidv4 } from "uuid";

interface Props {
  task: Task | null;
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

const STATUSES: Status[] = ["NotStarted", "InProgress", "OnHold", "Complete", "Dropped"];

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
  const [relaxed, setRelaxed] = useState(task?.relaxed_mode ?? false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [workerFilter, setWorkerFilter] = useState("");

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  // Compute forward dependents (tasks/milestones that depend on this task)
  const forwardDependents = task ? [
    ...Object.entries(plan.tasks)
      .filter(([, t]) => t.dependencies.some((d) =>
        typeof d.id === "object" && "Task" in d.id && d.id.Task === task.id
      ))
      .map(([, t]) => ({ id: t.id, name: t.name, type: "Task" as const })),
    ...Object.entries(plan.milestones)
      .filter(([, m]) => m.dependencies.some((d) =>
        typeof d.id === "object" && "Task" in d.id && d.id.Task === task.id
      ))
      .map(([, m]) => ({ id: m.id, name: m.name, type: "Milestone" as const })),
  ] : [];

  const filteredUsers = workerFilter.trim()
    ? users.filter((u) => u.name.toLowerCase().includes(workerFilter.toLowerCase()))
    : users;

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const dur = parseFloat(durationDays) || 0;
      const actualStartVal = actualStart || null;

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
            id: uuidv4(),
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

  const updateWorker = (idx: number, userId: UserId, workload: number) => {
    setWorkers((prev) => {
      const next = [...prev];
      next[idx] = { Specific: { user_id: userId, workload_days: workload } };
      return next;
    });
  };

  const removeWorker = (idx: number) => {
    setWorkers((prev) => prev.filter((_, i) => i !== idx));
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
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {STATUSES.map((s) => (
            <label
              key={s}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 4,
                fontSize: 12,
                cursor: "pointer",
                color: "#d4d4d4",
              }}
            >
              <input
                type="radio"
                name="task-status"
                checked={status === s}
                onChange={() => setStatus(s)}
              />
              {STATUS_LABELS[s]}
            </label>
          ))}
        </div>
      </div>

      {/* Duration */}
      <div className="form-row">
        <label>Duration (days, 0 = derive from workload)</label>
        <input
          type="number"
          min={0}
          step={0.5}
          value={durationDays}
          onChange={(e) => setDurationDays(e.target.value)}
          style={{ maxWidth: 120 }}
        />
      </div>

      {/* Relaxed mode */}
      <div className="form-row">
        <label
          style={{ display: "flex", alignItems: "center", gap: 8, cursor: "pointer" }}
        >
          <input
            type="checkbox"
            checked={relaxed}
            onChange={(e) => setRelaxed(e.target.checked)}
          />
          Relaxed mode (workers don't need to share same days)
        </label>
      </div>

      <ConstraintEditor value={constraint} onChange={setConstraint} />

      {/* Actual start */}
      <div className="form-row">
        <label>Actual Start</label>
        <input
          type="date"
          value={actualStart}
          onChange={(e) => setActualStart(e.target.value)}
          style={{ maxWidth: 200 }}
        />
      </div>

      {/* Workers */}
      <div className="form-row">
        <label>Workers</label>
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {workers.map((w, idx) => {
            const uid = workerUserId(w) ?? "";
            const wl = workerWorkload(w);
            return (
              <div key={idx} style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <select
                  value={uid}
                  onChange={(e) => updateWorker(idx, e.target.value, wl)}
                  style={{
                    flex: 1,
                    background: "#1e1e1e",
                    border: "1px solid #3a3a3c",
                    borderRadius: 4,
                    color: "#d4d4d4",
                    fontSize: 12,
                    padding: "4px 8px",
                    outline: "none",
                  }}
                >
                  {users.map((u) => (
                    <option key={u.id} value={u.id}>
                      {u.name}
                    </option>
                  ))}
                </select>
                <input
                  type="number"
                  min={0}
                  step={0.5}
                  value={wl}
                  title="Workload (days)"
                  onChange={(e) =>
                    updateWorker(idx, uid, parseFloat(e.target.value) || 0)
                  }
                  style={{
                    width: 70,
                    background: "#1e1e1e",
                    border: "1px solid #3a3a3c",
                    borderRadius: 4,
                    color: "#d4d4d4",
                    fontSize: 12,
                    padding: "3px 6px",
                    outline: "none",
                  }}
                />
                <span style={{ fontSize: 11, color: "#666" }}>days</span>
                <button
                  onClick={() => removeWorker(idx)}
                  style={{
                    background: "none",
                    border: "none",
                    color: "#888",
                    cursor: "pointer",
                    fontSize: 14,
                  }}
                >
                  ×
                </button>
              </div>
            );
          })}
          {users.length > 0 && (
            <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
              <input
                type="text"
                placeholder="Filter users…"
                value={workerFilter}
                onChange={(e) => setWorkerFilter(e.target.value)}
                style={{ fontSize: 12, padding: "3px 8px", background: "#1e1e1e", border: "1px solid #3a3a3c", borderRadius: 4, color: "#d4d4d4", outline: "none" }}
              />
              <button
                className="btn btn-secondary btn-sm"
                onClick={() => {
                  const first = filteredUsers[0];
                  if (first) setWorkers((prev) => [...prev, { Specific: { user_id: first.id, workload_days: 1 } }]);
                }}
                disabled={filteredUsers.length === 0}
                style={{ alignSelf: "flex-start" }}
              >
                + Add {filteredUsers.length === 1 ? filteredUsers[0].name : "Worker"}
              </button>
            </div>
          )}
        </div>
      </div>

      <DependencyEditor
        label="Dependencies"
        deps={dependencies}
        plan={plan}
        excludeNodeId={task ? { Task: task.id } : null}
        onChange={setDependencies}
      />

      {/* Forward dependents (read-only) */}
      {forwardDependents.length > 0 && (
        <div className="form-row">
          <label>Dependents</label>
          <div style={{ maxHeight: 120, overflowY: "auto", display: "flex", flexDirection: "column", gap: 2 }}>
            {forwardDependents.map((d) => (
              <div key={d.id} style={{ fontSize: 12, color: "#aaa", padding: "2px 0" }}>
                {d.type === "Milestone" ? "◇ " : "■ "}{d.name}
              </div>
            ))}
          </div>
        </div>
      )}

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
