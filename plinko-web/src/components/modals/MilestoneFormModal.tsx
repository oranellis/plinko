import { useState } from "react";
import { Modal } from "../Modal";
import type { DateConstraint, Dependency, Milestone, MilestoneId, NodeId, Plan, PlanRequest, PlanResponse, Task, TaskId } from "../../protocol";
import type { MilestonePatch } from "../../protocol";
import { DependencyEditor } from "./shared/DependencyEditor";
import { ConstraintEditor } from "./shared/ConstraintEditor";
import { v4 as uuidv4 } from "uuid";

function nodeKey(n: NodeId): string {
  if (n === "PlanStart") return "PlanStart";
  if (typeof n === "object" && "Task" in n) return `task:${n.Task}`;
  if (typeof n === "object" && "Milestone" in n) return `milestone:${n.Milestone}`;
  return String(n);
}

interface Props {
  milestone: Milestone | null;
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

function buildInitialDependents(milestone: Milestone | null, plan: Plan): Dependency[] {
  if (!milestone) return [];
  const result: Dependency[] = [];
  for (const t of Object.values(plan.tasks) as Task[]) {
    for (const d of t.dependencies) {
      if (typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === milestone.id) {
        result.push({ id: { Task: t.id as TaskId }, lag_days: d.lag_days });
      }
    }
  }
  for (const m of Object.values(plan.milestones) as Milestone[]) {
    if (m.id === milestone.id) continue;
    for (const d of m.dependencies) {
      if (typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === milestone.id) {
        result.push({ id: { Milestone: m.id as MilestoneId }, lag_days: d.lag_days });
      }
    }
  }
  return result;
}

export function MilestoneFormModal({ milestone, plan, sendRequest, onClose }: Props) {
  const [name, setName] = useState(milestone?.name ?? "");
  const [description, setDescription] = useState(milestone?.description ?? "");
  const [constraint, setConstraint] = useState<DateConstraint | null>(milestone?.constraint ?? null);
  const [dependencies, setDependencies] = useState<Dependency[]>(milestone?.dependencies ?? []);
  const [dependents, setDependents] = useState<Dependency[]>(() => buildInitialDependents(milestone, plan));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const thisNodeId: NodeId | null = milestone ? { Milestone: milestone.id } : null;

  const dependentKeys = new Set(dependents.map((d) => nodeKey(d.id)));

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const newMsId = milestone?.id ?? uuidv4();
      const newNodeId: NodeId = { Milestone: newMsId as MilestoneId };
      // Ensure there's always at least PlanStart as a dependency.
      const effectiveDeps: typeof dependencies =
        dependencies.length === 0 ? [{ id: "PlanStart", lag_days: 0 }] : dependencies;

      if (milestone) {
        const patch: MilestonePatch = {
          name: name.trim(),
          description,
          constraint,
          dependencies: effectiveDeps,
        };
        const resp = await sendRequest({ UpdateMilestone: [milestone.id, patch] });
        if (typeof resp === "object" && "Error" in resp) {
          setError(JSON.stringify(resp.Error));
          return;
        }
      } else {
        const resp = await sendRequest({
          CreateMilestone: {
            id: newMsId,
            name: name.trim(),
            description,
            constraint,
            dependencies: effectiveDeps,
            context_label: null,
          },
        });
        if (typeof resp === "object" && "Error" in resp) {
          setError(JSON.stringify(resp.Error));
          return;
        }
      }

      // Sync forward dependents
      const oldDependents = buildInitialDependents(milestone, plan);
      const oldKeys = new Set(oldDependents.map((d) => nodeKey(d.id)));
      const newDepMap = new Map(dependents.map((d) => [nodeKey(d.id), d]));
      const newKeys = new Set(newDepMap.keys());

      for (const old of oldDependents) {
        const k = nodeKey(old.id);
        if (!newKeys.has(k)) {
          if (typeof old.id === "object" && "Task" in old.id) {
            const t = plan.tasks[old.id.Task];
            if (t) await sendRequest({ UpdateTask: [t.id, {
              dependencies: t.dependencies.filter((d) => nodeKey(d.id) !== nodeKey(newNodeId)),
            }] });
          } else if (typeof old.id === "object" && "Milestone" in old.id) {
            const m = plan.milestones[old.id.Milestone];
            if (m) await sendRequest({ UpdateMilestone: [m.id, {
              dependencies: m.dependencies.filter((d) => nodeKey(d.id) !== nodeKey(newNodeId)),
            }] });
          }
        }
      }
      for (const dep of dependents) {
        const k = nodeKey(dep.id);
        if (!oldKeys.has(k)) {
          if (typeof dep.id === "object" && "Task" in dep.id) {
            const t = plan.tasks[dep.id.Task];
            if (t) await sendRequest({ UpdateTask: [t.id, {
              dependencies: [...t.dependencies, { id: newNodeId, lag_days: dep.lag_days }],
            }] });
          } else if (typeof dep.id === "object" && "Milestone" in dep.id) {
            const m = plan.milestones[dep.id.Milestone];
            if (m) await sendRequest({ UpdateMilestone: [m.id, {
              dependencies: [...m.dependencies, { id: newNodeId, lag_days: dep.lag_days }],
            }] });
          }
        }
      }

      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!milestone) return;
    setSaving(true);
    try {
      await sendRequest({ DeleteMilestone: milestone.id });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal
      title={milestone ? milestone.name : "New Milestone"}
      onClose={onClose}
      width={480}
    >
      {error && (
        <div style={{ color: "#e57373", fontSize: 13, marginBottom: 12 }}>{error}</div>
      )}
      <div className="form-row">
        <label>Name</label>
        <input
          type="text"
          value={name}
          autoFocus={!milestone}
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

      <ConstraintEditor value={constraint} onChange={setConstraint} />

      <DependencyEditor
        label="Dependencies"
        deps={dependencies}
        plan={plan}
        excludeNodeId={thisNodeId}
        onChange={setDependencies}
      />

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
        {milestone && (
          <button className="btn btn-danger" onClick={handleDelete} disabled={saving}>
            Delete
          </button>
        )}
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
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
