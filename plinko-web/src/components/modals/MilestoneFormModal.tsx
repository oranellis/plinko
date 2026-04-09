import { useState } from "react";
import { Modal } from "../Modal";
import type { DateConstraint, Dependency, Milestone, MilestonePatch, Plan, PlanRequest, PlanResponse } from "../../protocol";
import { DependencyEditor } from "./shared/DependencyEditor";
import { ConstraintEditor } from "./shared/ConstraintEditor";
import { v4 as uuidv4 } from "uuid";

interface Props {
  milestone: Milestone | null;
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function MilestoneFormModal({ milestone, plan, sendRequest, onClose }: Props) {
  const [name, setName] = useState(milestone?.name ?? "");
  const [description, setDescription] = useState(milestone?.description ?? "");
  const [constraint, setConstraint] = useState<DateConstraint | null>(milestone?.constraint ?? null);
  const [dependencies, setDependencies] = useState<Dependency[]>(milestone?.dependencies ?? []);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      if (milestone) {
        const patch: MilestonePatch = {
          name: name.trim(),
          description,
          constraint,
          dependencies,
        };
        const resp = await sendRequest({ UpdateMilestone: [milestone.id, patch] });
        if (typeof resp === "object" && "Error" in resp) {
          setError(JSON.stringify(resp.Error));
          return;
        }
      } else {
        const resp = await sendRequest({
          CreateMilestone: {
            id: uuidv4(),
            name: name.trim(),
            description,
            constraint,
            dependencies,
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
    if (!milestone) return;
    setSaving(true);
    try {
      await sendRequest({ DeleteMilestone: milestone.id });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  // Compute forward dependents
  const forwardDependents = milestone ? [
    ...Object.entries(plan.tasks)
      .filter(([, t]) => t.dependencies.some((d) =>
        typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === milestone.id
      ))
      .map(([, t]) => ({ id: t.id, name: t.name, type: "Task" as const })),
    ...Object.entries(plan.milestones)
      .filter(([, m]) => m.id !== milestone.id && m.dependencies.some((d) =>
        typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === milestone.id
      ))
      .map(([, m]) => ({ id: m.id, name: m.name, type: "Milestone" as const })),
  ] : [];

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
        excludeNodeId={milestone ? { Milestone: milestone.id } : null}
        onChange={setDependencies}
      />

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
