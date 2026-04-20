import { useCallback, useEffect, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { PlanPermissionsModal } from "../components/modals/PlanPermissionsModal";
import type { AuthUser, BugReport, OrgMember, Organisation, OrgRole, PlanError } from "../protocol";
import { formatPlanError } from "../protocol";
import "./AdminPage.css";

export function AdminPage() {
  const { auth, sendRequest, setPage, status } = usePlanContext();
  const isSiteAdmin = !!auth.currentUser?.isAdmin;

  // Redirect non-admins away
  useEffect(() => {
    if (auth.currentUser && !isSiteAdmin) {
      setPage("home");
    }
  }, [auth.currentUser, isSiteAdmin, setPage]);

  // ── Auth Users ─────────────────────────────────────────────────────────────
  const [authUsers, setAuthUsers] = useState<AuthUser[]>([]);
  const [authUsersLoaded, setAuthUsersLoaded] = useState(false);
  const [newUserEmail, setNewUserEmail] = useState("");
  const [newUserPassword, setNewUserPassword] = useState("");
  const [newUserIsAdmin, setNewUserIsAdmin] = useState(false);
  const [createUserError, setCreateUserError] = useState<string | null>(null);
  const [editPasswordId, setEditPasswordId] = useState<string | null>(null);
  const [editPasswordValue, setEditPasswordValue] = useState("");
  const [editPasswordError, setEditPasswordError] = useState<string | null>(null);

  const fetchAuthUsers = useCallback(() => {
    sendRequest("GetAuthUsers").then((resp) => {
      if (typeof resp === "object" && resp !== null && "AuthUsers" in resp) {
        setAuthUsers((resp as { AuthUsers: AuthUser[] }).AuthUsers);
        setAuthUsersLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest]);

  useEffect(() => {
    if (status !== "connected" || !isSiteAdmin) return;
    fetchAuthUsers();
  }, [status, isSiteAdmin, fetchAuthUsers]);

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
    if (!confirm("Delete this user? This cannot be undone.")) return;
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

  // ── Organisations ──────────────────────────────────────────────────────────
  const [orgs, setOrgs] = useState<Organisation[]>([]);
  const [selectedOrgId, setSelectedOrgId] = useState<string | null>(null);
  const [orgMembers, setOrgMembers] = useState<OrgMember[]>([]);
  const [orgMembersLoaded, setOrgMembersLoaded] = useState(false);
  const [orgNewMemberUserId, setOrgNewMemberUserId] = useState("");
  const [orgNewMemberRole, setOrgNewMemberRole] = useState<OrgRole>("User");
  const [orgAddError, setOrgAddError] = useState<string | null>(null);
  const [orgRenameValue, setOrgRenameValue] = useState("");
  const [newOrgName, setNewOrgName] = useState("");
  const [createOrgError, setCreateOrgError] = useState<string | null>(null);
  const [planPermissionsUser, setPlanPermissionsUser] = useState<{ userId: string; email: string } | null>(null);

  const fetchOrgs = useCallback(() => {
    sendRequest("ListOrganisations").then((resp) => {
      if (typeof resp === "object" && resp !== null && "OrgList" in resp) {
        const list = (resp as { OrgList: Organisation[] }).OrgList;
        setOrgs(list);
        if (list.length > 0 && !selectedOrgId) setSelectedOrgId(list[0].id);
      }
    }).catch(console.error);
  }, [sendRequest, selectedOrgId]);

  useEffect(() => {
    if (status !== "connected" || !isSiteAdmin) return;
    fetchOrgs();
  }, [status, isSiteAdmin, fetchOrgs]);

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
    // eslint-disable-next-line react-hooks/set-state-in-effect
    fetchOrgMembers(selectedOrgId);
    const org = orgs.find((o) => o.id === selectedOrgId);
    setOrgRenameValue(org?.name ?? "");
  }, [selectedOrgId, fetchOrgMembers, orgs]);

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

  const handleRenameOrg = async () => {
    if (!selectedOrgId || !orgRenameValue.trim()) return;
    const resp = await sendRequest({ RenameOrganisation: { org_id: selectedOrgId, name: orgRenameValue.trim() } });
    if (resp === "PlanUpdated") {
      setOrgs((prev) => prev.map((o) => o.id === selectedOrgId ? { ...o, name: orgRenameValue.trim() } : o));
    }
  };

  const handleDeleteOrg = async (orgId: string) => {
    if (!confirm("Delete this organisation? All member associations will be removed.")) return;
    const resp = await sendRequest({ DeleteOrganisation: { org_id: orgId } });
    if (resp === "PlanUpdated") {
      setOrgs((prev) => prev.filter((o) => o.id !== orgId));
      if (selectedOrgId === orgId) setSelectedOrgId(orgs.find((o) => o.id !== orgId)?.id ?? null);
    } else if (typeof resp === "object" && resp !== null && "Error" in resp) {
      alert(formatPlanError((resp as { Error: PlanError }).Error));
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

  // ── Bug Reports ────────────────────────────────────────────────────────────
  const [bugReports, setBugReports] = useState<BugReport[]>([]);
  const [bugReportsLoaded, setBugReportsLoaded] = useState(false);

  const fetchBugReports = useCallback(() => {
    sendRequest("ListBugReports").then((resp) => {
      if (typeof resp === "object" && resp !== null && "BugReports" in resp) {
        setBugReports((resp as { BugReports: BugReport[] }).BugReports);
        setBugReportsLoaded(true);
      }
    }).catch(console.error);
  }, [sendRequest]);

  useEffect(() => {
    if (status !== "connected" || !isSiteAdmin) return;
    fetchBugReports();
  }, [status, isSiteAdmin, fetchBugReports]);

  const selectedOrg = orgs.find((o) => o.id === selectedOrgId);

  if (!isSiteAdmin) {
    return (
      <div className="admin-page">
        <div className="admin-error">Access denied. Site administrators only.</div>
      </div>
    );
  }

  return (
    <div className="admin-page">
      {planPermissionsUser && selectedOrgId && (
        <PlanPermissionsModal
          orgId={selectedOrgId}
          userId={planPermissionsUser.userId}
          email={planPermissionsUser.email}
          sendRequest={sendRequest}
          onClose={() => setPlanPermissionsUser(null)}
        />
      )}

      <div className="admin-header">
        <h1 className="admin-title">Site Administration</h1>
        <button className="btn btn-secondary btn-sm" onClick={() => setPage("settings")}>← Back to Settings</button>
      </div>

      <div className="admin-body">

        {/* ── Users ── */}
        <section className="admin-section">
          <h2 className="admin-section-title">Login Users</h2>

          <div className="admin-list">
            {!authUsersLoaded ? (
              <div className="admin-empty">Loading…</div>
            ) : authUsers.length === 0 ? (
              <div className="admin-empty">No users.</div>
            ) : authUsers.map((u) => (
              <div key={u.id} className="admin-list-row" style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span style={{ flex: 1, fontSize: 13, color: "#d4d4d4" }}>{u.email}</span>
                  {u.is_admin && <span className="home-user-badge">admin</span>}
                  {u.id !== auth.currentUser?.userId ? (
                    <>
                      <button className="btn btn-secondary btn-sm" onClick={() => handleToggleAdmin(u.id, !u.is_admin)}>
                        {u.is_admin ? "Remove admin" : "Make admin"}
                      </button>
                      <button className="btn btn-secondary btn-sm" onClick={() => {
                        setEditPasswordId(editPasswordId === u.id ? null : u.id);
                        setEditPasswordValue("");
                        setEditPasswordError(null);
                      }}>
                        Set password
                      </button>
                      <button className="btn btn-danger btn-sm" onClick={() => handleDeleteUser(u.id)}>Delete</button>
                    </>
                  ) : (
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
                    <button className="btn btn-primary btn-sm" onClick={() => handleSetPassword(u.id)} disabled={!editPasswordValue}>Save</button>
                    {editPasswordError && <span style={{ color: "#e57373", fontSize: 12 }}>{editPasswordError}</span>}
                  </div>
                )}
              </div>
            ))}
          </div>

          <h3 className="admin-subheading">Create New User</h3>
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
        </section>

        {/* ── Organisations ── */}
        <section className="admin-section">
          <h2 className="admin-section-title">Organisations</h2>

          <h3 className="admin-subheading">Create Organisation</h3>
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
            <button className="btn btn-primary" onClick={handleCreateOrg} disabled={!newOrgName.trim()}>Create Organisation</button>
          </div>

          {orgs.length > 0 && (
            <>
              <h3 className="admin-subheading">Select Organisation</h3>
              <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 16 }}>
                {orgs.map((org) => (
                  <div key={org.id} style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <button
                      className={`settings-org-tab${selectedOrgId === org.id ? " active" : ""}`}
                      onClick={() => setSelectedOrgId(org.id)}
                      style={{ flex: 1 }}
                    >
                      {org.name}
                    </button>
                    <button className="btn btn-danger btn-sm" onClick={() => handleDeleteOrg(org.id)}>Delete</button>
                  </div>
                ))}
              </div>

              {selectedOrg && (
                <>
                  <div className="form-row">
                    <label>Rename "{selectedOrg.name}"</label>
                    <div style={{ display: "flex", gap: 8 }}>
                      <input
                        type="text"
                        value={orgRenameValue}
                        onChange={(e) => setOrgRenameValue(e.target.value)}
                        onKeyDown={(e) => { if (e.key === "Enter") handleRenameOrg(); }}
                        style={{ flex: 1, background: "#1e1e1e", border: "1px solid #3a3a3c", borderRadius: 4, color: "#d4d4d4", fontSize: 13, padding: "6px 10px", outline: "none", fontFamily: "inherit" }}
                      />
                      <button className="btn btn-secondary btn-sm" onClick={handleRenameOrg} disabled={!orgRenameValue.trim()}>Rename</button>
                    </div>
                  </div>

                  <h3 className="admin-subheading">Members of {selectedOrg.name}</h3>
                  <div className="admin-list" style={{ marginBottom: 12 }}>
                    {!orgMembersLoaded ? (
                      <div className="admin-empty">Loading…</div>
                    ) : orgMembers.length === 0 ? (
                      <div className="admin-empty">No members yet.</div>
                    ) : orgMembers.map((m) => (
                      <div key={m.user_id} className="admin-list-row">
                        <span style={{ flex: 1, fontSize: 13, color: "#d4d4d4" }}>{m.email}</span>
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
                        <button className="btn btn-danger btn-sm" onClick={() => handleRemoveOrgMember(m.user_id)}>Remove</button>
                      </div>
                    ))}
                  </div>

                  <h3 className="admin-subheading">Add Member</h3>
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
                    <button className="btn btn-primary btn-sm" onClick={handleAddOrgMember} disabled={!orgNewMemberUserId}>Add</button>
                  </div>
                  {orgAddError && <span className="settings-error" style={{ marginTop: 6 }}>{orgAddError}</span>}
                </>
              )}
            </>
          )}
        </section>

        {/* ── Bug Reports ── */}
        <section className="admin-section">
          <h2 className="admin-section-title">Bug Reports</h2>
          <div style={{ marginBottom: 8 }}>
            <button className="btn btn-secondary btn-sm" onClick={fetchBugReports}>Refresh</button>
          </div>
          {!bugReportsLoaded ? (
            <div className="admin-empty">Loading…</div>
          ) : bugReports.length === 0 ? (
            <div className="admin-empty">No bug reports submitted.</div>
          ) : (
            <div className="admin-bug-table-wrap">
              <table className="admin-bug-table">
                <thead>
                  <tr>
                    <th>#</th>
                    <th>User</th>
                    <th>Submitted</th>
                    <th>Page</th>
                    <th>Description</th>
                    <th>User Agent</th>
                  </tr>
                </thead>
                <tbody>
                  {bugReports.map((r) => (
                    <tr key={r.id}>
                      <td style={{ color: "#888", fontSize: 11 }}>{r.id}</td>
                      <td>{r.user_email}</td>
                      <td style={{ whiteSpace: "nowrap", fontSize: 12 }}>{r.submitted_at.replace("T", " ").slice(0, 19)}</td>
                      <td style={{ fontSize: 12, maxWidth: 160, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.page_url}</td>
                      <td style={{ maxWidth: 300 }}>{r.description}</td>
                      <td style={{ fontSize: 11, color: "#888", maxWidth: 200, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{r.user_agent}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>

      </div>
    </div>
  );
}
