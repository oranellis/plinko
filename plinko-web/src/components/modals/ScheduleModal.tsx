import { useState } from "react";
import { Modal } from "../Modal";
import type { Plan, PlanRequest, PlanResponse, UserId, Weekday, WorkSchedule } from "../../protocol";

const WEEKDAYS: Weekday[] = [
  "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

interface Props {
  userId: UserId | null; // null = plan default schedule
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function ScheduleModal({ userId, plan, sendRequest, onClose }: Props) {
  const baseSchedule = userId
    ? (plan.users_data[userId]?.schedule ?? plan.default_schedule)
    : plan.default_schedule;

  const [hours, setHours] = useState<Partial<Record<Weekday, string>>>(
    Object.fromEntries(
      WEEKDAYS.map((wd) => [wd, String(baseSchedule.days[wd] ?? 0)])
    ) as Partial<Record<Weekday, string>>
  );
  const [saving, setSaving] = useState(false);

  const title = userId
    ? `${plan.users_data[userId]?.user.name ?? "User"} Schedule`
    : "Default Schedule";

  const buildSchedule = (): WorkSchedule => ({
    days: Object.fromEntries(
      WEEKDAYS.map((wd) => [wd, parseFloat(hours[wd] ?? "0") || 0])
    ) as WorkSchedule["days"],
  });

  const handleSave = async () => {
    setSaving(true);
    try {
      const schedule = buildSchedule();
      if (userId) {
        await sendRequest({ SetUserSchedule: [userId, schedule] });
      } else {
        await sendRequest({ SetDefaultSchedule: schedule });
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleReset = async () => {
    if (!userId) return;
    setSaving(true);
    try {
      await sendRequest({ ClearUserSchedule: userId });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title={title} onClose={onClose} width={360}>
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        {WEEKDAYS.map((wd) => (
          <div key={wd} style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ width: 100, fontSize: 13, color: "#d4d4d4" }}>{wd}</span>
            <input
              type="number"
              min={0}
              max={24}
              step={0.5}
              value={hours[wd] ?? "0"}
              onChange={(e) => setHours((prev) => ({ ...prev, [wd]: e.target.value }))}
              style={{
                width: 70,
                background: "#1e1e1e",
                border: "1px solid #3a3a3c",
                borderRadius: 4,
                color: "#d4d4d4",
                fontSize: 13,
                padding: "4px 8px",
                outline: "none",
              }}
            />
            <span style={{ fontSize: 12, color: "#666" }}>hours</span>
          </div>
        ))}
      </div>
      <div className="form-actions" style={{ marginTop: 20 }}>
        {userId && (
          <button className="btn btn-secondary" onClick={handleReset} disabled={saving}>
            Reset to Default
          </button>
        )}
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button className="btn btn-primary" onClick={handleSave} disabled={saving}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}
