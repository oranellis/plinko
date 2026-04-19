import { useState } from "react";
import { Modal } from "../Modal";
import type { Organisation, PlanRequest, PlanResponse } from "../../protocol";
import { todayIso } from "../../utils/planUtils";
import { DatePicker } from "./shared/DatePicker";

interface Props {
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
  orgs: Organisation[];
}

export function NewPlanModal({ sendRequest, onClose, orgs }: Props) {
  const [name, setName] = useState("");
  const [startDate, setStartDate] = useState(todayIso());
  const [orgId, setOrgId] = useState<string>(orgs[0]?.id ?? "");
  const [saving, setSaving] = useState(false);

  const handleCreate = async () => {
    if (!name.trim()) return;
    setSaving(true);
    try {
      await sendRequest({ NewPlan: { org_id: orgId || null } });
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
      {orgs.length > 0 && (
        <div className="form-row">
          <label>Organisation</label>
          <select
            value={orgId}
            onChange={(e) => setOrgId(e.target.value)}
            style={{ background: "#1e1e1e", border: "1px solid #3a3a3c", borderRadius: 4, color: "#d4d4d4", fontSize: 13, padding: "6px 10px", width: "100%" }}
          >
            <option value="">— None —</option>
            {orgs.map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
          </select>
        </div>
      )}
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
