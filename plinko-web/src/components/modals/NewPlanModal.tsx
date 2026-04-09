import { useState } from "react";
import { Modal } from "../Modal";
import type { PlanRequest, PlanResponse } from "../../protocol";
import { todayIso } from "../../utils/planUtils";
import { DatePicker } from "./shared/DatePicker";

interface Props {
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function NewPlanModal({ sendRequest, onClose }: Props) {
  const [name, setName] = useState("");
  const [startDate, setStartDate] = useState(todayIso());
  const [saving, setSaving] = useState(false);

  const handleCreate = async () => {
    if (!name.trim()) return;
    setSaving(true);
    try {
      await sendRequest("NewPlan");
      await sendRequest({
        UpdatePlanSettings: {
          name: name.trim(),
          start_date: startDate,
          scheduler_target: "PlanStart",
        },
      });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title="New Plan" onClose={onClose} width={400}>
      <div className="form-row">
        <label>Plan Name</label>
        <input
          type="text"
          value={name}
          autoFocus
          placeholder="My Plan"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
        />
      </div>
      <div className="form-row">
        <label>Start Date</label>
        <DatePicker value={startDate} onChange={setStartDate} placeholder="Plan start date…" />
      </div>
      <div className="form-actions">
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button
          className="btn btn-primary"
          onClick={handleCreate}
          disabled={saving || !name.trim()}
        >
          {saving ? "Creating…" : "Create"}
        </button>
      </div>
    </Modal>
  );
}
