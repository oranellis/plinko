import { useCallback, useEffect, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { MondayModal } from "../components/modals/MondayModal";
import { NewPlanModal } from "../components/modals/NewPlanModal";
import { DatePicker } from "../components/modals/shared/DatePicker";
import { nodeIdString } from "../utils/planUtils";
import type { AuthUser, NodeId, UserLink } from "../protocol";
import "./SettingsPage.css";

interface PlanEntry {
  id: string;
  name: string;
  timestamp: string;
}

export function SettingsPage() {
  const { plan, status, auth, sendRequest } = usePlanContext();
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [showMonday, setShowMonday] = useState(false);
  const [showNewPlan, setShowNewPlan] = useState(false);
  const [loadingId, setLoadingId] = useState<string | null>(null);
  const [fetchError, setFetchError] = useState<string | null>(null);

  // Auth user management state
  const [authUsers, setAuthUsers] = useState<AuthUser[]>([]);
  const [authUsersLoaded, setAuthUsersLoaded] = useState(false);
  const [newUserEmail, setNewUserEmail] = useState("");
  const [newUserPassword, setNewUserPassword] = useState("");
  const [newUserIsAdmin, setNewUserIsAdmin] = useState(false);
  const [createUserError, setCreateUserError] = useState<string | null>(null);
  const [editPasswordId, setEditPasswordId] = useState<string | null>(null);
  const [editPasswordValue, setEditPasswordValue] = useState("");
  const [editPasswordError, setEditPasswordError] = useState<string | null>(null);

  // User links state (login user → plan user)
  const [userLinks, setUserLinks] = useState<UserLink[]>([]);
  const [userLinksLoaded, setUserLinksLoaded] = useState(false);

  // Plan visibility state (admin only)
  const [planVisibility, setPlanVisibility] = useState<Record<string, string[]>>({});
  const [visibilityLoaded, setVisibilityLoaded] = useState(false);

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
    ...Object.values(plan.tasks ?? {}).map((t) => ({ key: `task:${t.id}`, label: t.name })),
    ...Object.values(plan.milestones ?? {}).map((m) => ({ key: `milestone:${m.id}`, label: m.name })),
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

  // Auth users
  const fetchAuthUsers = useCallback(() => {
    sendRequest("GetAuthUsers").then((resp) => {
      if (typeof resp === "object" && resp !== null && "AuthUsers" in resp) {
        setAuthUsers((resp as { AuthUsers: AuthUser[] }).AuthUsers);
        setAuthUsersLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest]);

  useEffect(() => {
    if (status !== "connected" || !auth.currentUser?.isAdmin) return;
    fetchAuthUsers();
  }, [status, auth.currentUser?.isAdmin, fetchAuthUsers]);

  const handleCreateUser = async () => {
    setCreateUserError(null);
    try {
      const resp = await sendRequest({ CreateAuthUser: { email: newUserEmail, password: newUserPassword, is_admin: newUserIsAdmin } });
      if (typeof resp === "object" && resp !== null && "AuthUserCreated" in resp) {
        setNewUserEmail("");
        setNewUserPassword("");
        setNewUserIsAdmin(false);
        fetchAuthUsers();
      } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
        const err = resp as { Error: { message: string } };
        setCreateUserError(err.Error.message);
      }
    } catch (e) {
      setCreateUserError(String(e));
    }
  };

  const handleToggleAdmin = async (userId: string, isAdmin: boolean) => {
    await sendRequest({ UpdateAuthUser: { user_id: userId, is_admin: isAdmin } });
    fetchAuthUsers();
  };

  const handleDeleteUser = async (userId: string) => {
    await sendRequest({ DeleteAuthUser: { user_id: userId } });
    fetchAuthUsers();
  };

  const handleSetPassword = async (userId: string) => {
    setEditPasswordError(null);
    try {
      const resp = await sendRequest({ SetAuthUserPassword: { user_id: userId, new_password: editPasswordValue } });
      if (typeof resp === "object" && resp !== null && "PasswordChanged" in resp) {
        setEditPasswordId(null);
        setEditPasswordValue("");
      } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
        const err = resp as { Error: { message: string } };
        setEditPasswordError(err.Error.message);
      }
    } catch (e) {
      setEditPasswordError(String(e));
    }
  };

  // User links
  const fetchUserLinks = useCallback(() => {
    if (!plan) return;
    sendRequest({ GetUserLinks: { plan_id: plan.id } }).then((resp) => {
      if (typeof resp === "object" && resp !== null && "UserLinks" in resp) {
        setUserLinks((resp as { UserLinks: UserLink[] }).UserLinks);
        setUserLinksLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest, plan]);

  useEffect(() => {
    if (status !== "connected" || !plan) return;
    fetchUserLinks();
  }, [status, plan, fetchUserLinks]);

  const handleSetUserLink = async (planUserId: string, loginUserId: string | null) => {
    if (!plan) return;
    const updated = userLinks.filter((l) => l.plan_user_id !== planUserId);
    if (loginUserId) updated.push({ login_user_id: loginUserId, plan_user_id: planUserId });
    setUserLinks(updated);
    await sendRequest({ SetUserLinks: { plan_id: plan.id, links: updated } });
  };

  // Plan visibility
  const fetchAllPlanVisibility = useCallback(async () => {
    if (!auth.currentUser?.isAdmin) return;
    const updated: Record<string, string[]> = {};
    await Promise.all(
      plans.map(async (p) => {
        const resp = await sendRequest({ GetPlanVisibility: { plan_id: p.id } });
        if (typeof resp === "object" && resp !== null && "PlanVisibility" in resp) {
          updated[p.id] = (resp as { PlanVisibility: { plan_id: string; user_ids: string[] } }).PlanVisibility.user_ids;
        } else {
          updated[p.id] = [];
        }
      })
    );
    setPlanVisibility(updated);
    setVisibilityLoaded(true);
  }, [sendRequest, plans, auth.currentUser?.isAdmin]);

  useEffect(() => {
    if (status !== "connected" || !auth.currentUser?.isAdmin || plans.length === 0) return;
    fetchAllPlanVisibility();
  }, [status, auth.currentUser?.isAdmin, plans, fetchAllPlanVisibility]);

  const handleTogglePlanVisibility = async (planId: string, userId: string) => {
    const current = planVisibility[planId] ?? [];
    const updated = current.includes(userId)
      ? current.filter((id) => id !== userId)
      : [...current, userId];
    setPlanVisibility((prev) => ({ ...prev, [planId]: updated }));
    await sendRequest({ SetPlanVisibility: { plan_id: planId, user_ids: updated } });
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

      {/* User Links — link plan users to login accounts */}
      {plan && userLinksLoaded && (
        <section className="settings-section">
          <h2 className="settings-heading">User Links</h2>
          <p className="settings-description">Link plan team members to login accounts.</p>
          {Object.values(plan.users_data ?? {}).length === 0 ? (
            <div className="settings-empty">No team members in this plan.</div>
          ) : (
            <div className="settings-plan-list">
              {Object.values(plan.users_data ?? {}).map(({ user: u }) => {
                const linked = userLinks.find((l) => l.plan_user_id === u.id);
                return (
                  <div key={u.id} className="settings-plan-row">
                    <span className="settings-plan-name">{u.name}</span>
                    <select
                      className="settings-select"
                      value={linked?.login_user_id ?? ""}
                      onChange={(e) => handleSetUserLink(u.id, e.target.value || null)}
                    >
                      <option value="">— unlinked —</option>
                      {authUsers.map((au) => (
                        <option key={au.id} value={au.id}>{au.email}</option>
                      ))}
                    </select>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      )}

      {/* Plan Access — admin only */}
      {auth.currentUser?.isAdmin && visibilityLoaded && authUsersLoaded && (
        <section className="settings-section">
          <h2 className="settings-heading">Plan Access</h2>
          <p className="settings-description">
            Control which login users can see each plan. If no users are selected, the plan is visible to everyone.
          </p>
          {plans.length === 0 ? (
            <div className="settings-empty">No plans found.</div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              {plans.map((p) => {
                const visibleTo = planVisibility[p.id] ?? [];
                const isPublic = visibleTo.length === 0;
                return (
                  <div key={p.id} style={{ background: "#1a1a2e", borderRadius: 8, padding: "12px 16px" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
                      <span style={{ fontWeight: 600, color: "#e0e0e0", flex: 1 }}>{p.name}</span>
                      {isPublic && <span style={{ fontSize: 11, color: "#888", background: "#2a2a3e", borderRadius: 4, padding: "2px 8px" }}>visible to all</span>}
                    </div>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                      {authUsers.filter((u) => !u.is_admin).map((u) => (
                        <label
                          key={u.id}
                          style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, color: "#ccc", cursor: "pointer" }}
                        >
                          <input
                            type="checkbox"
                            checked={visibleTo.includes(u.id)}
                            onChange={() => handleTogglePlanVisibility(p.id, u.id)}
                          />
                          {u.email}
                        </label>
                      ))}
                      {authUsers.filter((u) => !u.is_admin).length === 0 && (
                        <span style={{ color: "#666", fontSize: 13 }}>No non-admin users.</span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </section>
      )}

      {/* Login Users — admin only */}
      {auth.currentUser?.isAdmin && authUsersLoaded && (
        <section className="settings-section">
          <h2 className="settings-heading">Login Users</h2>

          <div className="settings-plan-list">
            {authUsers.map((u) => (
              <div key={u.id} className="settings-plan-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span className="settings-plan-name" style={{ flex: 1 }}>{u.email}</span>
                  {u.is_admin && <span className="home-user-badge">admin</span>}
                  {u.id !== auth.currentUser?.userId && (
                    <>
                      <button
                        className="btn btn-secondary btn-sm"
                        onClick={() => handleToggleAdmin(u.id, !u.is_admin)}
                      >
                        {u.is_admin ? "Remove admin" : "Make admin"}
                      </button>
                      <button
                        className="btn btn-secondary btn-sm"
                        onClick={() => { setEditPasswordId(editPasswordId === u.id ? null : u.id); setEditPasswordValue(""); setEditPasswordError(null); }}
                      >
                        Set password
                      </button>
                      <button
                        className="btn btn-danger btn-sm"
                        onClick={() => handleDeleteUser(u.id)}
                      >
                        Delete
                      </button>
                    </>
                  )}
                  {u.id === auth.currentUser?.userId && (
                    <span style={{ fontSize: 12, color: "#555" }}>(you)</span>
                  )}
                </div>
                {editPasswordId === u.id && (
                  <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
                    <input
                      type="password"
                      className="settings-input"
                      placeholder="New password"
                      value={editPasswordValue}
                      onChange={(e) => setEditPasswordValue(e.target.value)}
                      style={{ flex: 1 }}
                    />
                    <button
                      className="btn btn-primary btn-sm"
                      onClick={() => handleSetPassword(u.id)}
                      disabled={!editPasswordValue}
                    >
                      Save
                    </button>
                    {editPasswordError && <span style={{ color: "#e57373", fontSize: 12 }}>{editPasswordError}</span>}
                  </div>
                )}
              </div>
            ))}
          </div>

          <h3 className="settings-subheading">Create New User</h3>
          <div className="settings-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 10 }}>
            <input
              type="email"
              className="settings-input"
              placeholder="Email address"
              value={newUserEmail}
              onChange={(e) => setNewUserEmail(e.target.value)}
            />
            <input
              type="password"
              className="settings-input"
              placeholder="Password"
              value={newUserPassword}
              onChange={(e) => setNewUserPassword(e.target.value)}
            />
            <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, color: "#aaa", cursor: "pointer" }}>
              <input
                type="checkbox"
                checked={newUserIsAdmin}
                onChange={(e) => setNewUserIsAdmin(e.target.checked)}
              />
              Administrator
            </label>
            {createUserError && <span style={{ color: "#e57373", fontSize: 12 }}>{createUserError}</span>}
            <button
              className="btn btn-primary"
              onClick={handleCreateUser}
              disabled={!newUserEmail || !newUserPassword}
            >
              Create User
            </button>
          </div>
        </section>
      )}
    </div>
  );
}
