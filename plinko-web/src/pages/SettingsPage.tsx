import { useCallback, useEffect, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { MondayModal } from "../components/modals/MondayModal";
import { NewPlanModal } from "../components/modals/NewPlanModal";
import { DatePicker } from "../components/modals/shared/DatePicker";
import { nodeIdString } from "../utils/planUtils";
import type { NodeId } from "../protocol";
import "./SettingsPage.css";

interface PlanEntry {
  id: string;
  name: string;
  timestamp: string;
}

export function SettingsPage() {
  const { plan, status, sendRequest } = usePlanContext();
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [showMonday, setShowMonday] = useState(false);
  const [showNewPlan, setShowNewPlan] = useState(false);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [fetchError, setFetchError] = useState<string | null>(null);

  // Plan settings form state
  const [planName, setPlanName] = useState(plan?.name ?? "");
  const [planStartDate, setPlanStartDate] = useState(plan?.start_date ?? "");
  const [targetKey, setTargetKey] = useState(plan ? nodeIdString(plan.scheduler_target) : "plan_start");
  const [targetFilter, setTargetFilter] = useState("");
  const [targetOpen, setTargetOpen] = useState(false);
  const [planSaving, setPlanSaving] = useState(false);

  // Sync plan settings form when plan changes
  useEffect(() => {
    if (!plan) return;
    setPlanName(plan.name);
    setPlanStartDate(plan.start_date);
    setTargetKey(nodeIdString(plan.scheduler_target));
  }, [plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const nodeOptions: { key: string; label: string }[] = plan ? [
    { key: "plan_start", label: "Plan Start" },
    ...Object.values(plan.tasks).map((t) => ({ key: `task:${t.id}`, label: t.name })),
    ...Object.values(plan.milestones).map((m) => ({ key: `milestone:${m.id}`, label: m.name })),
  ].sort((a, b) => a.key === "plan_start" ? -1 : b.key === "plan_start" ? 1 : a.label.localeCompare(b.label)) : [];

  const resolveTarget = (key: string): NodeId => {
    if (key === "plan_start") return "PlanStart";
    if (key.startsWith("task:")) return { Task: key.slice(5) };
    return { Milestone: key.slice(10) };
  };

  const handlePlanSettingsSave = async () => {
    if (!plan) return;
    setPlanSaving(true);
    try {
      await sendRequest({
        UpdatePlanSettings: {
          name: planName,
          start_date: planStartDate,
          scheduler_target: resolveTarget(targetKey),
        },
      });
    } finally {
      setPlanSaving(false);
    }
  };

  const fetchPlans = useCallback(() => {
    setFetchError(null);
    sendRequest("ListPlans").then((resp) => {
      if (typeof resp === "object" && resp !== null && "PlanList" in resp) {
        setPlans(
          (resp as { PlanList: [string, string, string][] }).PlanList.map(
            ([id, name, ts]) => ({ id, name, timestamp: ts })
          )
        );
      } else {
        console.error("[SettingsPage] Unexpected ListPlans response:", resp);
        setFetchError("Unexpected response from server.");
      }
    }).catch((e: unknown) => {
      console.error("[SettingsPage] ListPlans failed:", e);
      setFetchError("Failed to fetch plans. Check server connection.");
    });
  }, [sendRequest]);

  // Fetch saved plans list whenever connected or the active plan changes
  useEffect(() => {
    if (status !== "connected") return;
    fetchPlans();
  }, [status, plan?.id, fetchPlans]);

  const handleSave = async () => {
    await sendRequest("SavePlan");
    fetchPlans();
  };

  const handleLoad = async (planId: string) => {
    setLoadingId(planId);
    try {
      await sendRequest({ LoadPlan: { plan_id: planId } });
    } finally {
      setLoadingId(null);
    }
  };

  const handleDelete = async (planId: string) => {
    await sendRequest({ DeletePlan: { plan_id: planId } });
    setPlans((prev) => prev.filter((p) => p.id !== planId));
  };

  return (
    <div className="settings-page">
      {showMonday && plan && (
        <MondayModal planId={plan.id} onClose={() => setShowMonday(false)} />
      )}
      {showNewPlan && (
        <NewPlanModal
          onClose={() => setShowNewPlan(false)}
          sendRequest={sendRequest}
        />
      )}

      {plan && (
        <section className="settings-section">
          <h2 className="settings-heading">Plan Settings</h2>
          <div className="form-row">
            <label>Plan Name</label>
            <input
              type="text"
              value={planName}
              onChange={(e) => setPlanName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handlePlanSettingsSave(); }}
            />
          </div>
          <div className="form-row">
            <label>Start Date</label>
            <DatePicker value={planStartDate} onChange={setPlanStartDate} placeholder="Plan start date…" />
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
                  borderRadius: 4, color: "#d4d4d4", fontSize: 12, padding: "4px 8px",
                  outline: "none", boxSizing: "border-box",
                }}
              />
              {targetOpen && (
                <div style={{
                  position: "absolute", top: "100%", left: 0, right: 0,
                  background: "#252526", border: "1px solid #3a3a3c", borderRadius: 4,
                  maxHeight: 180, overflowY: "auto", zIndex: 200,
                }}>
                  {nodeOptions
                    .filter((o) => !targetFilter || o.label.toLowerCase().includes(targetFilter.toLowerCase()))
                    .map((o) => (
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
          <div className="settings-row" style={{ marginTop: 12 }}>
            <button className="btn btn-primary" onClick={handlePlanSettingsSave} disabled={planSaving}>
              {planSaving ? "Saving…" : "Save Plan Settings"}
            </button>
          </div>
        </section>
      )}

      <section className="settings-section">
        <h2 className="settings-heading">Plan Management</h2>
        <div className="settings-row">
          <button className="btn btn-primary" onClick={handleSave}>
            Save Snapshot
          </button>
          <button
            className="btn btn-secondary"
            onClick={() => setShowNewPlan(true)}
          >
            New Plan
          </button>
        </div>

        <h3 className="settings-subheading">Saved Plans</h3>
        <div className="settings-row" style={{ marginBottom: 8 }}>
          <button className="btn btn-secondary btn-sm" onClick={fetchPlans} disabled={status !== "connected"}>
            Refresh
          </button>
          {fetchError && <span style={{ color: "#e57373", fontSize: 12 }}>{fetchError}</span>}
        </div>
        <div className="settings-plan-list">
          {plans.length === 0 && (
            <div className="settings-empty">No saved plans</div>
          )}
          {plans.map((p) => (
            <div key={p.id} className="settings-plan-row">
              <div className="settings-plan-info">
                <span className="settings-plan-name">{p.name}</span>
                <span className="settings-plan-ts">{p.timestamp}</span>
              </div>
              <div className="settings-plan-actions">
                <button
                  className="btn btn-secondary btn-sm"
                  onClick={() => handleLoad(p.id)}
                  disabled={loadingId === p.id}
                >
                  {loadingId === p.id ? "Loading…" : "Load"}
                </button>
                <button
                  className="btn btn-danger btn-sm"
                  onClick={() => handleDelete(p.id)}
                >
                  Delete
                </button>
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="settings-section">
        <button
          className="btn btn-secondary settings-monday-btn"
          onClick={() => setShowMonday(true)}
        >
          Monday.com Integration
        </button>
      </section>
    </div>
  );
}
