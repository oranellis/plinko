import { useEffect, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { MondayModal } from "../components/modals/MondayModal";
import { NewPlanModal } from "../components/modals/NewPlanModal";
import "./SettingsPage.css";

interface PlanEntry {
  id: string;
  name: string;
  timestamp: string;
}

export function SettingsPage() {
  const { plan, sendRequest } = usePlanContext();
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [showMonday, setShowMonday] = useState(false);
  const [showNewPlan, setShowNewPlan] = useState(false);
  const [loadingId, setLoadingId] = useState<string | null>(null);

  // Fetch saved plans list on mount
  useEffect(() => {
    sendRequest("ListPlans").then((resp) => {
      if (typeof resp === "object" && "PlanList" in resp) {
        setPlans(
          resp.PlanList.map(([id, name, ts]) => ({ id, name, timestamp: ts }))
        );
      }
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [plan?.id]);

  const handleSave = async () => {
    await sendRequest("SavePlan");
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

  const handleSetUser = async (userId: string | null) => {
    await sendRequest({ SetCurrentUser: userId });
  };

  const users = plan ? Object.values(plan.users_data).map((ud) => ud.user) : [];
  users.sort((a, b) => a.name.localeCompare(b.name));

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

      <section className="settings-section">
        <h2 className="settings-heading">Plan Management</h2>
        <div className="settings-row">
          <button className="btn btn-primary" onClick={handleSave}>
            Save Plan
          </button>
          <button
            className="btn btn-secondary"
            onClick={() => setShowNewPlan(true)}
          >
            New Plan
          </button>
        </div>

        <h3 className="settings-subheading">Saved Plans</h3>
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

      <section className="settings-section">
        <h2 className="settings-heading">Identity</h2>
        <div className="settings-identity-list">
          <label className="settings-identity-row">
            <input
              type="radio"
              name="identity"
              onChange={() => handleSetUser(null)}
              defaultChecked
            />
            <span>(No user — plan-wide view)</span>
          </label>
          {users.map((u) => (
            <label key={u.id} className="settings-identity-row">
              <input
                type="radio"
                name="identity"
                onChange={() => handleSetUser(u.id)}
              />
              <span>{u.name}</span>
            </label>
          ))}
        </div>
      </section>
    </div>
  );
}
