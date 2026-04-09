import { useState } from "react";
import { Modal } from "../Modal";
import { Plan, PlanRequest, PlanResponse } from "../../protocol";
import { nodeIdString } from "../../utils/planUtils";

interface Props {
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function PlanSettingsModal({ plan, sendRequest, onClose }: Props) {
  const [name, setName] = useState(plan.name);
  const [startDate, setStartDate] = useState(plan.start_date);
  const [targetKey, setTargetKey] = useState(nodeIdString(plan.scheduler_target));
  const [saving, setSaving] = useState(false);

  const nodeOptions: { key: string; label: string }[] = [
    { key: "plan_start", label: "Plan Start" },
    ...Object.values(plan.tasks).map((t) => ({
      key: `task:${t.id}`,
      label: t.name,
    })),
    ...Object.values(plan.milestones).map((m) => ({
      key: `milestone:${m.id}`,
      label: m.name,
    })),
  ];
  nodeOptions.sort((a, b) => (a.key === "plan_start" ? -1 : b.key === "plan_start" ? 1 : a.label.localeCompare(b.label)));

  const resolveTarget = (key: string) => {
    if (key === "plan_start") return "PlanStart" as const;
    if (key.startsWith("task:")) return { Task: key.slice(5) };
    return { Milestone: key.slice(10) };
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await sendRequest({
        UpdatePlanSettings: {
          name,
          start_date: startDate,
          scheduler_target: resolveTarget(targetKey),
        },
      });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title="Plan Settings" onClose={onClose} width={440}>
      <div className="form-row">
        <label>Plan Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleSave(); }}
        />
      </div>
      <div className="form-row">
        <label>Start Date</label>
        <input
          type="date"
          value={startDate}
          onChange={(e) => setStartDate(e.target.value)}
        />
      </div>
      <div className="form-row">
        <label>Scheduler Target</label>
        <select value={targetKey} onChange={(e) => setTargetKey(e.target.value)}>
          {nodeOptions.map((o) => (
            <option key={o.key} value={o.key}>{o.label}</option>
          ))}
        </select>
      </div>
      <div className="form-actions">
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}
