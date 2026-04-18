import { useCallback, useEffect, useMemo, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { MondayModal } from "../components/modals/MondayModal";
import { NewPlanModal } from "../components/modals/NewPlanModal";
import { DatePicker } from "../components/modals/shared/DatePicker";
import { nodeIdString } from "../utils/planUtils";
import type { AuthUser, NodeId, OrgMember, Organisation, OrgRole, PlanError, UserLink } from "../protocol";
import { formatPlanError } from "../protocol";
import "./SettingsPage.css";

type SettingsSection = "profile" | "plan" | "plan-management" | "user-links" | "organisation" | "site-admin";

interface PlanEntry {
  id: string;
  name: string;
  timestamp: string;
}

export function SettingsPage() {
  const { plan, status, auth, sendRequest } = usePlanContext();

  const isSiteAdmin = !!auth.currentUser?.isAdmin;
  const isOrgAdmin = auth.currentUser?.orgMemberships?.some((m) => m.role === "Admin") ?? false;

  // ── Sidebar navigation ──────────────────────────────────────────────────────
  const [activeSection, setActiveSection] = useState<SettingsSection>("profile");

  const sections = useMemo(() => {
    const items: { id: SettingsSection; label: string }[] = [
      { id: "profile", label: "Profile" },
    ];
    if (plan) items.push({ id: "plan", label: "Plan Settings" });
    items.push({ id: "plan-management", label: "Plan Management" });
    if (plan) items.push({ id: "user-links", label: "User Links" });
    if (isOrgAdmin || isSiteAdmin) items.push({ id: "organisation", label: "Organisation" });
    if (isSiteAdmin) items.push({ id: "site-admin", label: "Site Administration" });
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
  const [loadingId, setLoadingId] = useState<string | null>(null);
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

  const handleLoad = async (planId: string) => {
    setLoadingId(planId);
    try {
      await sendRequest({ LoadPlan: { plan_id: planId } });
    } finally {
      setLoadingId(null);
    }
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
  const [authUsersLoaded, setAuthUsersLoaded] = useState(false);

  const fetchAuthUsers = useCallback(() => {
    sendRequest("GetAuthUsers").then((resp) => {
      if (typeof resp === "object" && resp !== null && "AuthUsers" in resp) {
        setAuthUsers((resp as { AuthUsers: AuthUser[] }).AuthUsers);
        setAuthUsersLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest]);

  useEffect(() => {
    if (status !== "connected") return;
    fetchAuthUsers();
  }, [status, fetchAuthUsers]);

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
  const [orgs, setOrgs] = useState<Organisation[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string | null>(null);
  const [orgMembers, setOrgMembers] = useState<OrgMember[]>([]);
  const [orgMembersLoaded, setOrgMembersLoaded] = useState(false);
  const [currentPlanOrgId, setCurrentPlanOrgId] = useState<string | null | undefined>(undefined);
  const [orgNewMemberUserId, setOrgNewMemberUserId] = useState("");
  const [orgNewMemberRole, setOrgNewMemberRole] = useState<OrgRole>("User");
  const [orgAddError, setOrgAddError] = useState<string | null>(null);
  const [orgRenameValue, setOrgRenameValue] = useState("");
  const [newOrgName, setNewOrgName] = useState("");
  const [createOrgError, setCreateOrgError] = useState<string | null>(null);

  const fetchOrgs = useCallback(() => {
    sendRequest("ListOrganisations").then((resp) => {
      if (typeof resp === "object" && resp !== null && "OrgList" in resp) {
        const list = (resp as { OrgList: Organisation[] }).OrgList;
        setOrgs(list);
        if (list.length > 0 && !selectedOrgId) {
          setSelectedOrgId(list[0].id);
        }
      }
    }).catch(console.error);
  }, [sendRequest, selectedOrgId]);

  useEffect(() => {
    if (status !== "connected" || (!isOrgAdmin && !isSiteAdmin)) return;
    fetchOrgs();
  }, [status, isOrgAdmin, isSiteAdmin, fetchOrgs]);

  const fetchOrgMembers = useCallback((orgId: string) => {
    setOrgMembersLoaded(false);
    sendRequest({ GetOrgMembers: { org_id: orgId } }).then((resp) => {
      if (typeof resp === "object" && resp !== null && "OrgMembers" in resp) {
        setOrgMembers((resp as { OrgMembers: OrgMember[] }).OrgMembers);
        setOrgMembersLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest]);

  useEffect(() => {
    if (!selectedOrgId) return;
    fetchOrgMembers(selectedOrgId);
    // Reset rename value to selected org name
    const org = orgs.find((o) => o.id === selectedOrgId);
    setOrgRenameValue(org?.name ?? "");
  }, [selectedOrgId, fetchOrgMembers, orgs]);

  // Fetch current plan's org assignment
  useEffect(() => {
    if (!plan) { setCurrentPlanOrgId(undefined); return; }
    sendRequest({ GetPlanOrg: { plan_id: plan.id } }).then((resp) => {
      if (typeof resp === "object" && resp !== null && "PlanOrgId" in resp) {
        setCurrentPlanOrgId((resp as { PlanOrgId: string | null }).PlanOrgId);
      }
    }).catch(console.error);
  }, [plan?.id, sendRequest]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleRenameOrg = async () => {
    if (!selectedOrgId || !orgRenameValue.trim()) return;
    const resp = await sendRequest({ RenameOrganisation: { org_id: selectedOrgId, name: orgRenameValue.trim() } });
    if (resp === "PlanUpdated") {
      setOrgs((prev) => prev.map((o) => o.id === selectedOrgId ? { ...o, name: orgRenameValue.trim() } : o));
    }
  };

  const handleAddOrgMember = async () => {
    if (!selectedOrgId || !orgNewMemberUserId) return;
    setOrgAddError(null);
    const resp = await sendRequest({ AddOrgMember: { org_id: selectedOrgId, user_id: orgNewMemberUserId, role: orgNewMemberRole } });
    if (resp === "PlanUpdated") {
      setOrgNewMemberUserId("");
      fetchOrgMembers(selectedOrgId);
    } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
      setOrgAddError(formatPlanError((resp as { Error: PlanError }).Error));
    }
  };

  const handleRemoveOrgMember = async (userId: string) => {
    if (!selectedOrgId) return;
    await sendRequest({ RemoveOrgMember: { org_id: selectedOrgId, user_id: userId } });
    fetchOrgMembers(selectedOrgId);
  };

  const handleSetMemberRole = async (userId: string, role: OrgRole) => {
    if (!selectedOrgId) return;
    await sendRequest({ AddOrgMember: { org_id: selectedOrgId, user_id: userId, role } });
    fetchOrgMembers(selectedOrgId);
  };

  const handleSetPlanOrg = async (orgId: string | null) => {
    if (!plan) return;
    await sendRequest({ SetPlanOrg: { plan_id: plan.id, org_id: orgId } });
    setCurrentPlanOrgId(orgId);
  };

  const handleCreateOrg = async () => {
    if (!newOrgName.trim()) return;
    setCreateOrgError(null);
    const resp = await sendRequest({ CreateOrganisation: { name: newOrgName.trim() } });
    if (typeof resp === "object" && resp !== null && "OrgCreated" in resp) {
      setNewOrgName("");
      fetchOrgs();
    } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
      setCreateOrgError(formatPlanError((resp as { Error: PlanError }).Error));
    }
  };

  const handleDeleteOrg = async (orgId: string) => {
    if (!confirm("Delete this organisation? All member associations will be removed. Plans must be unassigned first.")) return;
    const resp = await sendRequest({ DeleteOrganisation: { org_id: orgId } });
    if (resp === "PlanUpdated") {
      setOrgs((prev) => prev.filter((o) => o.id !== orgId));
      if (selectedOrgId === orgId) setSelectedOrgId(orgs.find((o) => o.id !== orgId)?.id ?? null);
    } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
      alert(formatPlanError((resp as { Error: PlanError }).Error));
    }
  };

  // ── Site admin state ────────────────────────────────────────────────────────
  const [newUserEmail, setNewUserEmail] = useState("");
  const [newUserPassword, setNewUserPassword] = useState("");
  const [newUserIsAdmin, setNewUserIsAdmin] = useState(false);
  const [createUserError, setCreateUserError] = useState<string | null>(null);
  const [editPasswordId, setEditPasswordId] = useState<string | null>(null);
  const [editPasswordValue, setEditPasswordValue] = useState("");
  const [editPasswordError, setEditPasswordError] = useState<string | null>(null);
  const [planVisibility, setPlanVisibility] = useState<Record<string, string[]>>({});
  const [visibilityLoaded, setVisibilityLoaded] = useState(false);

  useEffect(() => {
    if (status !== "connected" || !isSiteAdmin) return;
    fetchAuthUsers();
  }, [status, isSiteAdmin, fetchAuthUsers]);

  const fetchAllPlanVisibility = useCallback(async () => {
    if (!isSiteAdmin) return;
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
  }, [sendRequest, plans, isSiteAdmin]);

  useEffect(() => {
    if (status !== "connected" || !isSiteAdmin || plans.length === 0) return;
    fetchAllPlanVisibility();
  }, [status, isSiteAdmin, plans, fetchAllPlanVisibility]);

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
        setCreateUserError(formatPlanError((resp as { Error: PlanError }).Error));
      }
    } catch (e) {
      setCreateUserError(String(e));
    }
  };

  const handleToggleAdmin = async (userId: string, isAdmin: boolean) => {
    await sendRequest({ UpdateAuthUser: { user_id: userId, new_is_admin: isAdmin } });
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
      if (resp === "PasswordChanged") {
        setEditPasswordId(null);
        setEditPasswordValue("");
      } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
        setEditPasswordError(formatPlanError((resp as { Error: PlanError }).Error));
      }
    } catch (e) {
      setEditPasswordError(String(e));
    }
  };

  const handleTogglePlanVisibility = async (planId: string, userId: string) => {
    const current = planVisibility[planId] ?? [];
    const updated = current.includes(userId) ? current.filter((id) => id !== userId) : [...current, userId];
    setPlanVisibility((prev) => ({ ...prev, [planId]: updated }));
    await sendRequest({ SetPlanVisibility: { plan_id: planId, user_ids: updated } });
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
        <button className="btn btn-secondary" onClick={() => setShowNewPlan(true)}>
          New Plan
        </button>
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
            <div className="settings-plan-actions">
              <button className="btn btn-secondary btn-sm" onClick={() => handleLoad(p.id)} disabled={loadingId === p.id}>
                {loadingId === p.id ? "Loading…" : "Load"}
              </button>
              <button className="btn btn-danger btn-sm" onClick={() => handleDelete(p.id)}>Delete</button>
            </div>
          </div>
        ))}
      </div>

      {plan && (
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
    const selectedOrg = orgs.find((o) => o.id === selectedOrgId);
    const myOrgMembership = auth.currentUser?.orgMemberships?.find((m) => m.org_id === selectedOrgId);
    const canManage = isSiteAdmin || myOrgMembership?.role === "Admin";

    return (
      <div className="settings-content-panel">
        <h2 className="settings-heading">Organisation</h2>

        {isSiteAdmin && (
          <>
            <h3 className="settings-subheading">Create Organisation</h3>
            <div className="settings-form-stack">
              <input
                type="text"
                className="settings-input"
                placeholder="Organisation name…"
                value={newOrgName}
                onChange={(e) => setNewOrgName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleCreateOrg(); }}
              />
              {createOrgError && <span className="settings-error">{createOrgError}</span>}
              <button className="btn btn-primary" onClick={handleCreateOrg} disabled={!newOrgName.trim()}>
                Create Organisation
              </button>
            </div>
          </>
        )}

        {orgs.length === 0 ? (
          <div className="settings-empty" style={{ marginTop: 24 }}>No organisations found.</div>
        ) : (
          <>
            {orgs.length > 1 || isSiteAdmin ? (
              <>
                <h3 className="settings-subheading">Select Organisation</h3>
                <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 20 }}>
                  {orgs.map((org) => (
                    <div key={org.id} style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <button
                        className={`settings-org-tab${selectedOrgId === org.id ? " active" : ""}`}
                        onClick={() => setSelectedOrgId(org.id)}
                        style={{ flex: 1 }}
                      >
                        {org.name}
                      </button>
                      {isSiteAdmin && (
                        <button className="btn btn-danger btn-sm" onClick={() => handleDeleteOrg(org.id)}>Delete</button>
                      )}
                    </div>
                  ))}
                </div>
              </>
            ) : null}

            {selectedOrg && (
              <>
                <h3 className="settings-subheading">{selectedOrg.name}</h3>

                {canManage && (
                  <>
                    <div className="form-row">
                      <label>Rename Organisation</label>
                      <div style={{ display: "flex", gap: 8 }}>
                        <input
                          type="text"
                          value={orgRenameValue}
                          onChange={(e) => setOrgRenameValue(e.target.value)}
                          onKeyDown={(e) => { if (e.key === "Enter") handleRenameOrg(); }}
                          style={{ flex: 1, background: "#1e1e1e", border: "1px solid #3a3a3c", borderRadius: 4, color: "#d4d4d4", fontSize: 13, padding: "6px 10px", outline: "none", fontFamily: "inherit" }}
                        />
                        <button className="btn btn-secondary btn-sm" onClick={handleRenameOrg} disabled={!orgRenameValue.trim()}>
                          Rename
                        </button>
                      </div>
                    </div>

                    {plan && currentPlanOrgId !== undefined && (
                      <div className="form-row">
                        <label>Plan Assignment</label>
                        {currentPlanOrgId === selectedOrgId ? (
                          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                            <span style={{ fontSize: 13, color: "#7dbd7d" }}>✓ "{plan.name}" is assigned to this organisation</span>
                            <button className="btn btn-secondary btn-sm" onClick={() => handleSetPlanOrg(null)}>Unassign</button>
                          </div>
                        ) : (
                          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                            <span style={{ fontSize: 13, color: "#888" }}>"{plan.name}" is not assigned to this organisation</span>
                            <button className="btn btn-primary btn-sm" onClick={() => handleSetPlanOrg(selectedOrgId)}>
                              Assign to this org
                            </button>
                          </div>
                        )}
                      </div>
                    )}
                  </>
                )}

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
                            <button className="btn btn-danger btn-sm" onClick={() => handleRemoveOrgMember(m.user_id)}>
                              Remove
                            </button>
                          </>
                        ) : (
                          <span style={{ fontSize: 12, color: "#888" }}>{m.role}</span>
                        )}
                      </div>
                    ))
                  )}
                </div>

                {canManage && (
                  <>
                    <h3 className="settings-subheading">Add Member</h3>
                    <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
                      <select
                        className="settings-select"
                        value={orgNewMemberUserId}
                        onChange={(e) => setOrgNewMemberUserId(e.target.value)}
                        style={{ flex: 1, minWidth: 160 }}
                      >
                        <option value="">Select user…</option>
                        {authUsersLoaded && authUsers
                          .filter((u) => !orgMembers.some((m) => m.user_id === u.id))
                          .map((u) => <option key={u.id} value={u.id}>{u.email}</option>)}
                      </select>
                      <select
                        className="settings-select"
                        value={orgNewMemberRole}
                        onChange={(e) => setOrgNewMemberRole(e.target.value as OrgRole)}
                        style={{ width: 90 }}
                      >
                        <option value="Admin">Admin</option>
                        <option value="User">User</option>
                        <option value="Viewer">Viewer</option>
                      </select>
                      <button className="btn btn-primary btn-sm" onClick={handleAddOrgMember} disabled={!orgNewMemberUserId}>
                        Add
                      </button>
                    </div>
                    {orgAddError && <span className="settings-error" style={{ marginTop: 6 }}>{orgAddError}</span>}
                  </>
                )}
              </>
            )}
          </>
        )}
      </div>
    );
  };

  const renderSiteAdmin = () => (
    <div className="settings-content-panel">
      <h2 className="settings-heading">Site Administration</h2>

      <h3 className="settings-subheading">Login Users</h3>
      <div className="settings-plan-list" style={{ marginBottom: 16 }}>
        {authUsers.map((u) => (
          <div key={u.id} className="settings-plan-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <span className="settings-plan-name" style={{ flex: 1 }}>{u.email}</span>
              {u.is_admin && <span className="home-user-badge">admin</span>}
              {u.id !== auth.currentUser?.userId && (
                <>
                  <button className="btn btn-secondary btn-sm" onClick={() => handleToggleAdmin(u.id, !u.is_admin)}>
                    {u.is_admin ? "Remove admin" : "Make admin"}
                  </button>
                  <button className="btn btn-secondary btn-sm" onClick={() => { setEditPasswordId(editPasswordId === u.id ? null : u.id); setEditPasswordValue(""); setEditPasswordError(null); }}>
                    Set password
                  </button>
                  <button className="btn btn-danger btn-sm" onClick={() => handleDeleteUser(u.id)}>Delete</button>
                </>
              )}
              {u.id === auth.currentUser?.userId && <span style={{ fontSize: 12, color: "#555" }}>(you)</span>}
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
                <button className="btn btn-primary btn-sm" onClick={() => handleSetPassword(u.id)} disabled={!editPasswordValue}>Save</button>
                {editPasswordError && <span style={{ color: "#e57373", fontSize: 12 }}>{editPasswordError}</span>}
              </div>
            )}
          </div>
        ))}
      </div>

      <h3 className="settings-subheading">Create New User</h3>
      <div className="settings-form-stack">
        <input type="email" className="settings-input" placeholder="Email address" value={newUserEmail} onChange={(e) => setNewUserEmail(e.target.value)} />
        <input type="password" className="settings-input" placeholder="Password" value={newUserPassword} onChange={(e) => setNewUserPassword(e.target.value)} />
        <label style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13, color: "#aaa", cursor: "pointer" }}>
          <input type="checkbox" checked={newUserIsAdmin} onChange={(e) => setNewUserIsAdmin(e.target.checked)} />
          Site Administrator
        </label>
        {createUserError && <span className="settings-error">{createUserError}</span>}
        <button className="btn btn-primary" onClick={handleCreateUser} disabled={!newUserEmail || !newUserPassword}>Create User</button>
      </div>

      {visibilityLoaded && authUsersLoaded && plans.length > 0 && (
        <>
          <h3 className="settings-subheading">Plan Access</h3>
          <p className="settings-description">Control which users can see each plan. Empty selection = visible to all.</p>
          <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
            {plans.map((p) => {
              const visibleTo = planVisibility[p.id] ?? [];
              return (
                <div key={p.id} style={{ background: "#1a1a2e", borderRadius: 8, padding: "12px 16px" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 8 }}>
                    <span style={{ fontWeight: 600, color: "#e0e0e0", flex: 1 }}>{p.name}</span>
                    {visibleTo.length === 0 && <span style={{ fontSize: 11, color: "#888", background: "#2a2a3e", borderRadius: 4, padding: "2px 8px" }}>visible to all</span>}
                  </div>
                  <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                    {authUsers.filter((u) => !u.is_admin).map((u) => (
                      <label key={u.id} style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 13, color: "#ccc", cursor: "pointer" }}>
                        <input type="checkbox" checked={visibleTo.includes(u.id)} onChange={() => handleTogglePlanVisibility(p.id, u.id)} />
                        {u.email}
                      </label>
                    ))}
                    {authUsers.filter((u) => !u.is_admin).length === 0 && <span style={{ color: "#666", fontSize: 13 }}>No non-admin users.</span>}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}
    </div>
  );

  return (
    <div className="settings-page">
      {showMonday && plan && <MondayModal planId={plan.id} onClose={() => setShowMonday(false)} />}
      {showNewPlan && <NewPlanModal onClose={() => setShowNewPlan(false)} sendRequest={sendRequest} />}

      <div className="settings-layout">
        {/* Sidebar */}
        <nav className="settings-sidebar">
          {sections.map((s) => (
            <button
              key={s.id}
              className={`settings-nav-item${activeSection === s.id ? " active" : ""}`}
              onClick={() => setActiveSection(s.id)}
            >
              {s.label}
            </button>
          ))}
        </nav>

        {/* Content panel */}
        <div className="settings-main">
          {activeSection === "profile" && renderProfile()}
          {activeSection === "plan" && plan && renderPlanSettings()}
          {activeSection === "plan-management" && renderPlanManagement()}
          {activeSection === "user-links" && plan && renderUserLinks()}
          {activeSection === "organisation" && (isOrgAdmin || isSiteAdmin) && renderOrganisation()}
          {activeSection === "site-admin" && isSiteAdmin && renderSiteAdmin()}
        </div>
      </div>
    </div>
  );
}
