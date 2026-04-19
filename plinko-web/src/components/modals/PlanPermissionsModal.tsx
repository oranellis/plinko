import { useCallback, useEffect, useState } from "react";
import { Modal } from "../Modal";
import type { PlanPermissionEntry, PlanRequest, PlanResponse } from "../../protocol";

interface Props {
  orgId: string;
  userId: string;
  email: string;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function PlanPermissionsModal({ orgId, userId, email, sendRequest, onClose }: Props) {
  const [entries, setEntries] = useState<PlanPermissionEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [savingPlanId, setSavingPlanId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadPermissions = useCallback(() => {
    setLoading(true);
    setError(null);
    sendRequest({ GetUserPlanPermissions: { org_id: orgId, user_id: userId } })
      .then((resp) => {
        if (typeof resp === "object" && resp !== null && "UserPlanPermissions" in resp) {
          setEntries((resp as { UserPlanPermissions: PlanPermissionEntry[] }).UserPlanPermissions);
        } else {
          setError("Failed to load plan permissions.");
        }
      })
      .catch(() => setError("Failed to load plan permissions."))
      .finally(() => setLoading(false));
  }, [orgId, userId, sendRequest]);

  useEffect(() => { loadPermissions(); }, [loadPermissions]);

  const handlePermissionChange = async (planId: string, permission: string) => {
    setSavingPlanId(planId);
    try {
      await sendRequest({ SetUserPlanPermission: { plan_id: planId, user_id: userId, permission } });
      setEntries((prev) =>
        prev.map((e) => (e.plan_id === planId ? { ...e, permission } : e))
      );
    } catch {
      setError("Failed to update permission.");
    } finally {
      setSavingPlanId(null);
    }
  };

  return (
    <Modal title={`Plan Access — ${email}`} onClose={onClose}>
      {loading ? (
        <div style={{ padding: "24px 0", textAlign: "center", color: "#888" }}>Loading…</div>
      ) : error ? (
        <div style={{ color: "#e57373", padding: "12px 0" }}>{error}</div>
      ) : entries.length === 0 ? (
        <div style={{ color: "#888", padding: "12px 0" }}>No plans in this organisation.</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <p style={{ fontSize: 13, color: "#888", margin: "0 0 8px 0" }}>
            Set each plan's access level for this user. "Default" means the user's org role applies.
          </p>
          {entries.map((entry) => (
            <div
              key={entry.plan_id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "8px 12px",
                background: "#1a1a2e",
                borderRadius: 6,
              }}
            >
              <span style={{ flex: 1, fontSize: 14, color: "#e0e0e0" }}>{entry.plan_name}</span>
              <select
                value={entry.permission}
                disabled={savingPlanId === entry.plan_id}
                onChange={(e) => handlePermissionChange(entry.plan_id, e.target.value)}
                style={{
                  background: "#2a2a3e",
                  border: "1px solid #3a3a4c",
                  borderRadius: 4,
                  color: entry.permission === "NoAccess" ? "#e57373" : "#d4d4d4",
                  fontSize: 13,
                  padding: "4px 8px",
                  cursor: "pointer",
                }}
              >
                <option value="Default">Default (org role)</option>
                <option value="Viewer">Viewer</option>
                <option value="User">User</option>
                <option value="NoAccess">No Access</option>
              </select>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}
