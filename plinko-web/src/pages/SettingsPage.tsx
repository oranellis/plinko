import { useCallback, useEffect, useMemo, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { MondayModal } from "../components/modals/MondayModal";
import { NewPlanModal } from "../components/modals/NewPlanModal";
import { PlanPermissionsModal } from "../components/modals/PlanPermissionsModal";
import { DatePicker } from "../components/modals/shared/DatePicker";
import { nodeIdString } from "../utils/planUtils";
import type { AuthUser, NodeId, OrgMember, OrgRole, PlanError, UserLink } from "../protocol";
import { formatPlanError } from "../protocol";
import "./SettingsPage.css";

type SettingsSection = "profile" | "plan" | "plan-management" | "user-links" | "organisation";

interface PlanEntry {
  id: string;
  name: string;
  timestamp: string;
}

export function SettingsPage() {
  const { plan, status, auth, sendRequest, setPage, setActiveOrg } = usePlanContext();

  const isSiteAdmin = !!auth.currentUser?.isAdmin;
  const isOrgAdmin = auth.currentUser?.orgMemberships?.some((m) => m.role === "Admin") ?? false;

  // ── Sidebar navigation ──────────────────────────────────────────────────────
  const [activeSection, setActiveSection] = useState<SettingsSection>("profile");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(() => window.innerWidth < 600);

  const sections = useMemo(() => {
    const items: { id: SettingsSection; label: string }[] = [
      { id: "profile", label: "Profile" },
    ];
    if (plan) items.push({ id: "plan", label: "Plan Settings" });
    items.push({ id: "plan-management", label: "Plan Management" });
    if (plan && (isOrgAdmin || isSiteAdmin)) items.push({ id: "user-links", label: "User Links" });
    if (isOrgAdmin || isSiteAdmin) items.push({ id: "organisation", label: "Organisation" });
    return items;
  }, [plan, isOrgAdmin, isSiteAdmin]);

  // If active section disappears (e.g. plan unloaded), fall back to profile
  useEffect(() => {
    if (!sections.some((s) => s.id === activeSection)) {
      setActiveSection("profile");
    }
  }, [sections, activeSection]);

  // ── Profile state ───────────────────────────────────────────────────────────
  const [oldPassword, setOldPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [passwordError, setPasswordError] = useState<string | null>(null);
  const [passwordSuccess, setPasswordSuccess] = useState(false);

  const handleChangePassword = async () => {
    setPasswordError(null);
    setPasswordSuccess(false);
    if (newPassword !== confirmPassword) {
      setPasswordError("New passwords do not match.");
      return;
    }
    if (newPassword.length < 4) {
      setPasswordError("New password must be at least 4 characters.");
      return;
    }
    try {
      const resp = await sendRequest({ ChangeMyPassword: { old_password: oldPassword, new_password: newPassword } });
      if (resp === "PasswordChanged") {
        setOldPassword("");
        setNewPassword("");
        setConfirmPassword("");
        setPasswordSuccess(true);
      } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
        setPasswordError(formatPlanError((resp as { Error: PlanError }).Error));
      }
    } catch (e) {
      setPasswordError(String(e));
    }
  };

  // ── Plan settings state ─────────────────────────────────────────────────────
  const [planName, setPlanName] = useState(plan?.name ?? "");
  const [planStartDate, setPlanStartDate] = useState(plan?.start_date ?? "");
  const [targetKey, setTargetKey] = useState(plan ? nodeIdString(plan.scheduler_target) : "plan_start");
  const [targetFilter, setTargetFilter] = useState("");
  const [targetOpen, setTargetOpen] = useState(false);
  const [planSaving, setPlanSaving] = useState(false);

  useEffect(() => {
    if (!plan) return;
    setPlanName(plan.name);
    setPlanStartDate(plan.start_date);
    setTargetKey(nodeIdString(plan.scheduler_target));
  }, [plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const nodeOptions = plan
    ? [
        { key: "plan_start", label: "Plan Start" },
        ...Object.values(plan.tasks ?? {}).map((t) => ({ key: `task:${t.id}`, label: t.name })),
        ...Object.values(plan.milestones ?? {}).map((m) => ({ key: `milestone:${m.id}`, label: m.name })),
      ].sort((a, b) => (a.key === "plan_start" ? -1 : b.key === "plan_start" ? 1 : a.label.localeCompare(b.label)))
    : [];

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
        UpdatePlanSettings: { name: planName, start_date: planStartDate, scheduler_target: resolveTarget(targetKey) },
      });
    } finally {
      setPlanSaving(false);
    }
  };

  // ── Plan management state ───────────────────────────────────────────────────
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [showMonday, setShowMonday] = useState(false);
  const [showNewPlan, setShowNewPlan] = useState(false);
  const [fetchError, setFetchError] = useState<string | null>(null);
  const [showVersionModal, setShowVersionModal] = useState(false);
  const [versionList, setVersionList] = useState<string[]>([]);
  const [versionLoading, setVersionLoading] = useState(false);
  const [versionRestoring, setVersionRestoring] = useState<string | null>(null);

  const fetchPlans = useCallback(() => {
    setFetchError(null);
    sendRequest("ListPlans").then((resp) => {
      if (typeof resp === "object" && resp !== null && "PlanList" in resp) {
        setPlans((resp as { PlanList: [string, string, string][] }).PlanList.map(([id, name, ts]) => ({ id, name, timestamp: ts })));
      } else {
        setFetchError("Unexpected response from server.");
      }
    }).catch(() => setFetchError("Failed to fetch plans."));
  }, [sendRequest]);

  useEffect(() => {
    if (status !== "connected") return;
    fetchPlans();
  }, [status, plan?.id, fetchPlans]);

  const handleSave = async () => {
    await sendRequest("SavePlan");
    fetchPlans();
  };

  const handleDelete = async (planId: string) => {
    if (!confirm("Delete this plan? This cannot be undone.")) return;
    await sendRequest({ DeletePlan: { plan_id: planId } });
    setPlans((prev) => prev.filter((p) => p.id !== planId));
  };

  const handleOpenVersionHistory = async () => {
    if (!plan) return;
    setVersionLoading(true);
    setShowVersionModal(true);
    try {
      const resp = await sendRequest({ ListPlanVersions: { plan_id: plan.id } });
      if (typeof resp === "object" && resp !== null && "PlanVersionList" in resp) {
        setVersionList((resp as { PlanVersionList: string[] }).PlanVersionList.reverse());
      }
    } finally {
      setVersionLoading(false);
    }
  };

  const handleRestoreVersion = async (version: string) => {
    if (!plan) return;
    if (!confirm(`Restore version "${formatVersion(version)}"? Current state will be saved first.`)) return;
    setVersionRestoring(version);
    try {
      await sendRequest({ RestorePlanVersion: { plan_id: plan.id, version } });
      setShowVersionModal(false);
    } finally {
      setVersionRestoring(null);
    }
  };

  const formatVersion = (v: string) => v.replace("T", " ").replace(/-(\d{2})-(\d{2})$/, ":$1:$2");

  // ── User links state ────────────────────────────────────────────────────────
  const [userLinks, setUserLinks] = useState<UserLink[]>([]);
  const [userLinksLoaded, setUserLinksLoaded] = useState(false);
  const [authUsers, setAuthUsers] = useState<AuthUser[]>([]);

  const fetchAuthUsers = useCallback(() => {
    if (!plan) return;
    sendRequest({ GetAuthUsersForPlan: { plan_id: plan.id } }).then((resp) => {
      if (typeof resp === "object" && resp !== null && "AuthUsers" in resp) {
        setAuthUsers((resp as { AuthUsers: AuthUser[] }).AuthUsers);
      }
    }).catch(console.error);
  }, [sendRequest, plan]);

  useEffect(() => {
    if (status !== "connected" || !plan) return;
    fetchAuthUsers();
  }, [status, plan, fetchAuthUsers]);

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

  // ── Organisation state ──────────────────────────────────────────────────────
  // Always operate on the active org — derived directly from auth state, no extra request needed.
  const activeOrgId = auth.currentUser?.activeOrgId ?? null;
  const activeOrgName = auth.currentUser?.orgMemberships?.find(
    (m) => m.org_id === activeOrgId
  )?.org_name ?? null;

  const [orgMembers, setOrgMembers] = useState<OrgMember[]>([]);
  const [orgMembersLoaded, setOrgMembersLoaded] = useState(false);
  const [planPermissionsUser, setPlanPermissionsUser] = useState<{ userId: string; email: string } | null>(null);

  const fetchOrgMembers = useCallback(() => {
    if (!activeOrgId) return;
    setOrgMembersLoaded(false);
    sendRequest({ GetOrgMembers: { org_id: activeOrgId } }).then((resp) => {
      if (typeof resp === "object" && resp !== null && "OrgMembers" in resp) {
        setOrgMembers((resp as { OrgMembers: OrgMember[] }).OrgMembers);
        setOrgMembersLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest, activeOrgId]);

  useEffect(() => {
    if (status !== "connected" || (!isOrgAdmin && !isSiteAdmin)) return;
    fetchOrgMembers();
  }, [status, isOrgAdmin, isSiteAdmin, fetchOrgMembers]);

  const handleSetMemberRole = async (userId: string, role: OrgRole) => {
    if (!activeOrgId) return;
    await sendRequest({ AddOrgMember: { org_id: activeOrgId, user_id: userId, role } });
    fetchOrgMembers();
  };

  // ── Render helpers ──────────────────────────────────────────────────────────

  const renderProfile = () => (
    <div className="settings-content-panel">
      <h2 className="settings-heading">Profile</h2>
      <p className="settings-description">
        Signed in as <strong>{auth.currentUser?.email}</strong>
        {isSiteAdmin && <span className="home-user-badge" style={{ marginLeft: 8 }}>site admin</span>}
        {isOrgAdmin && !isSiteAdmin && <span className="home-user-badge" style={{ marginLeft: 8 }}>org admin</span>}
      </p>

      <h3 className="settings-subheading">Change Password</h3>
      <div className="settings-form-stack">
        <input
          type="password"
          className="settings-input"
          placeholder="Current password"
          value={oldPassword}
          onChange={(e) => setOldPassword(e.target.value)}
        />
        <input
          type="password"
          className="settings-input"
          placeholder="New password"
          value={newPassword}
          onChange={(e) => setNewPassword(e.target.value)}
        />
        <input
          type="password"
          className="settings-input"
          placeholder="Confirm new password"
          value={confirmPassword}
          onChange={(e) => setConfirmPassword(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleChangePassword(); }}
        />
        {passwordError && <span className="settings-error">{passwordError}</span>}
        {passwordSuccess && <span className="settings-success">Password changed successfully.</span>}
        <button
          className="btn btn-primary"
          onClick={handleChangePassword}
          disabled={!oldPassword || !newPassword || !confirmPassword}
        >
          Change Password
        </button>
      </div>
    </div>
  );

  const renderPlanSettings = () => (
    <div className="settings-content-panel">
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
            style={{ width: "100%", background: "#1e1e1e", border: "1px solid #3a3a3c", borderRadius: 4, color: "#d4d4d4", fontSize: 12, padding: "4px 8px", outline: "none", boxSizing: "border-box" }}
          />
          {targetOpen && (
            <div style={{ position: "absolute", top: "100%", left: 0, right: 0, background: "#252526", border: "1px solid #3a3a3c", borderRadius: 4, maxHeight: 180, overflowY: "auto", zIndex: 200 }}>
              {nodeOptions
                .filter((o) => !targetFilter || o.label.toLowerCase().includes(targetFilter.toLowerCase()))
                .map((o) => (
                  <button key={o.key} onMouseDown={() => { setTargetKey(o.key); setTargetOpen(false); }}
                    style={{ display: "block", width: "100%", textAlign: "left", background: o.key === targetKey ? "#2d4a6a" : "none", border: "none", padding: "6px 10px", color: "#d4d4d4", fontSize: 12, cursor: "pointer", fontFamily: "inherit" }}
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
    </div>
  );

  const renderPlanManagement = () => (
    <div className="settings-content-panel">
      <h2 className="settings-heading">Plan Management</h2>
      <div className="settings-row" style={{ marginBottom: 16 }}>
        <button className="btn btn-primary" onClick={handleSave} disabled={!plan}>
          Save Snapshot
        </button>
        {(isOrgAdmin || isSiteAdmin) && (
          <button className="btn btn-secondary" onClick={() => setShowNewPlan(true)}>
            New Plan
          </button>
        )}
      </div>

      <h3 className="settings-subheading">Saved Plans</h3>
      <div className="settings-row" style={{ marginBottom: 8 }}>
        <button className="btn btn-secondary btn-sm" onClick={fetchPlans} disabled={status !== "connected"}>Refresh</button>
        {fetchError && <span style={{ color: "#e57373", fontSize: 12 }}>{fetchError}</span>}
      </div>
      <div className="settings-plan-list">
        {plans.length === 0 && <div className="settings-empty">No saved plans</div>}
        {plans.map((p) => (
          <div key={p.id} className="settings-plan-row">
            <div className="settings-plan-info">
              <span className="settings-plan-name">{p.name}</span>
              <span className="settings-plan-ts">{p.timestamp}</span>
            </div>
            {(isOrgAdmin || isSiteAdmin) && (
              <div className="settings-plan-actions">
                <button className="btn btn-danger btn-sm" onClick={() => handleDelete(p.id)}>Delete</button>
              </div>
            )}
          </div>
        ))}
      </div>

      {plan && (isOrgAdmin || isSiteAdmin) && (
        <>
          <h3 className="settings-subheading">Integrations</h3>
          <div className="settings-row" style={{ gap: 8 }}>
            <button className="btn btn-secondary" style={{ flex: 1, maxWidth: 220 }} onClick={() => setShowMonday(true)}>
              Monday.com Integration
            </button>
            {isSiteAdmin && (
              <button className="btn btn-secondary" style={{ flex: 1, maxWidth: 220 }} onClick={handleOpenVersionHistory}>
                Version History…
              </button>
            )}
          </div>
        </>
      )}

      {/* Version history modal */}
      {showVersionModal && (
        <div className="modal-overlay" onClick={() => setShowVersionModal(false)}>
          <div className="modal" style={{ minWidth: 360, maxWidth: 480 }} onClick={(e) => e.stopPropagation()}>
            <div className="modal-header">
              <h3>Version History</h3>
              <button className="modal-close" onClick={() => setShowVersionModal(false)}>✕</button>
            </div>
            <div className="modal-body">
              {versionLoading ? (
                <div style={{ color: "#888", textAlign: "center", padding: 16 }}>Loading versions…</div>
              ) : versionList.length === 0 ? (
                <div style={{ color: "#888", textAlign: "center", padding: 16 }}>No saved versions found.</div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column", gap: 6, maxHeight: 360, overflowY: "auto" }}>
                  {versionList.map((v) => (
                    <div key={v} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 12px", background: "#1a1a2e", borderRadius: 6 }}>
                      <span style={{ fontFamily: "monospace", fontSize: 13, color: "#ccc" }}>{formatVersion(v)}</span>
                      <button className="btn btn-secondary" style={{ fontSize: 12, padding: "4px 12px" }} disabled={versionRestoring === v} onClick={() => handleRestoreVersion(v)}>
                        {versionRestoring === v ? "Restoring…" : "Restore"}
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );

  const renderUserLinks = () => (
    <div className="settings-content-panel">
      <h2 className="settings-heading">User Links</h2>
      <p className="settings-description">Link plan team members to their login accounts.</p>
      {!userLinksLoaded ? (
        <div className="settings-empty">Loading…</div>
      ) : Object.values(plan?.users_data ?? {}).length === 0 ? (
        <div className="settings-empty">No team members in this plan.</div>
      ) : (
        <div className="settings-plan-list">
          {Object.values(plan?.users_data ?? {}).map(({ user: u }) => {
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
    </div>
  );

  const renderOrganisation = () => {
    const myOrgMembership = auth.currentUser?.orgMemberships?.find((m) => m.org_id === activeOrgId);
    const canManage = isSiteAdmin || myOrgMembership?.role === "Admin";

    return (
      <div className="settings-content-panel">
        <h2 className="settings-heading">Organisation</h2>

        {!activeOrgId ? (
          <div className="settings-empty" style={{ marginTop: 24 }}>No organisation found.</div>
        ) : (
          <>
            <h3 className="settings-subheading">{activeOrgName}</h3>

            <h3 className="settings-subheading">Members</h3>
            <div className="settings-plan-list" style={{ marginBottom: 16 }}>
              {!orgMembersLoaded ? (
                <div className="settings-empty">Loading…</div>
              ) : orgMembers.length === 0 ? (
                <div className="settings-empty">No members yet.</div>
              ) : (
                orgMembers.map((m) => (
                  <div key={m.user_id} className="settings-plan-row">
                    <span className="settings-plan-name" style={{ flex: 1 }}>{m.email}</span>
                    {canManage ? (
                      <>
                        <select
                          className="settings-select"
                          value={m.role}
                          onChange={(e) => handleSetMemberRole(m.user_id, e.target.value as OrgRole)}
                          style={{ width: 90 }}
                        >
                          <option value="Admin">Admin</option>
                          <option value="User">User</option>
                          <option value="Viewer">Viewer</option>
                        </select>
                        <button
                          className="btn btn-secondary btn-sm"
                          onClick={() => setPlanPermissionsUser({ userId: m.user_id, email: m.email })}
                        >
                          Plan Access
                        </button>
                      </>
                    ) : (
                      <span style={{ fontSize: 12, color: "#888" }}>{m.role}</span>
                    )}
                  </div>
                ))
              )}
            </div>
          </>
        )}
      </div>
    );
  };



  return (
    <div className="settings-page">
      {showMonday && plan && <MondayModal planId={plan.id} onClose={() => setShowMonday(false)} />}
      {showNewPlan && activeOrgId && activeOrgName && (
        <NewPlanModal
          orgs={[{ id: activeOrgId, name: activeOrgName }]}
          onClose={() => setShowNewPlan(false)}
          sendRequest={sendRequest}
        />
      )}
      {planPermissionsUser && activeOrgId && (
        <PlanPermissionsModal
          orgId={activeOrgId}
          userId={planPermissionsUser.userId}
          email={planPermissionsUser.email}
          sendRequest={sendRequest}
          onClose={() => setPlanPermissionsUser(null)}
        />
      )}

      <div className="settings-layout">
        {/* Sidebar */}
        <nav className={`settings-sidebar${sidebarCollapsed ? " settings-sidebar--collapsed" : ""}`}>
          <button
            className="settings-sidebar-close"
            onClick={() => setSidebarCollapsed(true)}
            title="Hide navigation"
          >
            ✕
          </button>
          {sections.map((s) => (
            <button
              key={s.id}
              className={`settings-nav-item${activeSection === s.id ? " active" : ""}`}
              onClick={() => setActiveSection(s.id)}
            >
              {s.label}
            </button>
          ))}
          <div className="settings-sidebar-bottom">
            {auth.currentUser && auth.currentUser.orgMemberships && auth.currentUser.orgMemberships.length > 0 && (
              <div className="settings-org-selector">
                <label className="settings-org-selector-label">Organisation</label>
                <select
                  className="settings-select settings-org-selector-select"
                  value={auth.currentUser.activeOrgId ?? ""}
                  onChange={(e) => { setActiveOrg(e.target.value).then(() => setPage("home")); }}
                  disabled={auth.currentUser.orgMemberships.length <= 1}
                >
                  {auth.currentUser.orgMemberships.map((m) => (
                    <option key={m.org_id} value={m.org_id}>{m.org_name}</option>
                  ))}
                </select>
              </div>
            )}
            {isSiteAdmin && (
              <button
                className="settings-nav-item settings-admin-link"
                onClick={() => setPage("admin")}
              >
                Site Administration →
              </button>
            )}
          </div>
        </nav>

        {/* Content panel */}
        <div className="settings-main">
          {sidebarCollapsed && (
            <button
              className="settings-sidebar-open"
              onClick={() => setSidebarCollapsed(false)}
              title="Show navigation"
            >
              ☰
            </button>
          )}
          {activeSection === "profile" && renderProfile()}
          {activeSection === "plan" && plan && renderPlanSettings()}
          {activeSection === "plan-management" && renderPlanManagement()}
          {activeSection === "user-links" && plan && renderUserLinks()}
          {activeSection === "organisation" && (isOrgAdmin || isSiteAdmin) && renderOrganisation()}
        </div>
      </div>
    </div>
  );
}
