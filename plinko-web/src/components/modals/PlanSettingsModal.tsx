import { useState } from "react";
import { Modal } from "../Modal";
import type { Plan, PlanRequest, PlanResponse } from "../../protocol";
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
  const [targetFilter, setTargetFilter] = useState("");
  const [targetOpen, setTargetOpen] = useState(false);
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
        <div style={{ position: "relative" }}>
          <input
            type="text"
            value={targetOpen ? targetFilter : (nodeOptions.find((o) => o.key === targetKey)?.label ?? targetKey)}
            placeholder="Search nodes…"
            onFocus={() => { setTargetOpen(true); setTargetFilter(""); }}
            onBlur={() => setTimeout(() => setTargetOpen(false), 150)}
            onChange={(e) => setTargetFilter(e.target.value)}
            style={{
              width: "100%", background: "#1e1e1e", border: "1px solid #3a3a3c",
              borderRadius: 4, color: "#d4d4d4", fontSize: 12, padding: "4px 8px", outline: "none"
            }}
          />
          {targetOpen && (
            <div style={{
              position: "absolute", top: "100%", left: 0, right: 0,
              background: "#252526", border: "1px solid #3a3a3c", borderRadius: 4,
              maxHeight: 180, overflowY: "auto", zIndex: 200,
            }}>
              {nodeOptions.filter((o) => !targetFilter || o.label.toLowerCase().includes(targetFilter.toLowerCase())).map((o) => (
                <button key={o.key} onMouseDown={() => { setTargetKey(o.key); setTargetOpen(false); }}
                  style={{
                    display: "block", width: "100%", textAlign: "left",
                    background: o.key === targetKey ? "#2d4a6a" : "none",
                    border: "none", padding: "6px 10px", color: "#d4d4d4",
                    fontSize: 12, cursor: "pointer", fontFamily: "inherit",
                  }}
                >{o.label}</button>
              ))}
            </div>
          )}
        </div>
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
