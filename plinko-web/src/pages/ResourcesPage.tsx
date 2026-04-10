import { useState } from "react";
import { v4 as uuidv4 } from "uuid";
import { usePlanContext } from "../context/PlanContext";
import type { IsoDate, TagId, User, UserId } from "../protocol";
import { formatDate } from "../utils/planUtils";
import "./ResourcesPage.css";

// ── Calendar helpers ─────────────────────────────────────────────────────────

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

function weekdayMon(d: Date): number {
  return (d.getDay() + 6) % 7;
}

function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

// ── Sub-views ────────────────────────────────────────────────────────────────

type UserView = { type: "list" } | { type: "create" } | { type: "edit"; userId: UserId };

// ── ResourcesPage ────────────────────────────────────────────────────────────

export function ResourcesPage() {
  const { plan, sendRequest } = usePlanContext();

  // ── User management state ──
  const [userView, setUserView] = useState<UserView>({ type: "list" });
  const [selectedUserId, setSelectedUserId] = useState<UserId | null>(null);
  const [userName, setUserName] = useState("");
  const [userTags, setUserTags] = useState<Set<TagId>>(new Set());
  const [userSaving, setUserSaving] = useState(false);
  const [userError, setUserError] = useState<string | null>(null);

  // ── Calendar state ──
  const today = new Date();
  const [year, setYear] = useState(today.getFullYear());
  const [month, setMonth] = useState(today.getMonth());
  const [calUserId, setCalUserId] = useState<UserId | null>(null);
  const [editPopup, setEditPopup] = useState<{ date: IsoDate; current: number | null; value: string } | null>(null);

  // ── Tags state ──
  const [newTagName, setNewTagName] = useState("");
  const [renaming, setRenaming] = useState<Record<TagId, string>>({});

  if (!plan) {
    return (
      <div className="resources-page resources-page--empty">
        No plan loaded
      </div>
    );
  }

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  // ── User actions ──────────────────────────────────────────────────────────

  const openCreate = () => {
    setUserName("");
    setUserTags(new Set());
    setUserError(null);
    setUserView({ type: "create" });
  };

  const openEdit = (user: User) => {
    setUserName(user.name);
    setUserTags(new Set(user.tags));
    setUserError(null);
    setUserView({ type: "edit", userId: user.id });
  };

  const handleSaveUser = async () => {
    if (!userName.trim()) return;
    setUserSaving(true);
    setUserError(null);
    try {
      const tags = [...userTags];
      if (userView.type === "create") {
        const resp = await sendRequest({ CreateUser: { id: uuidv4(), name: userName.trim(), tags } });
        if (typeof resp === "object" && "Error" in resp) { setUserError(JSON.stringify(resp.Error)); return; }
      } else if (userView.type === "edit") {
        const resp = await sendRequest({ UpdateUser: [userView.userId, { name: userName.trim(), tags }] });
        if (typeof resp === "object" && "Error" in resp) { setUserError(JSON.stringify(resp.Error)); return; }
      }
      setUserView({ type: "list" });
    } catch (e) {
      setUserError(String(e));
    } finally {
      setUserSaving(false);
    }
  };

  const handleDeleteUser = async (userId: UserId) => {
    await sendRequest({ DeleteUser: userId });
    setUserView({ type: "list" });
  };

  const toggleUserTag = (tagId: TagId) => {
    setUserTags((prev) => {
      const next = new Set(prev);
      if (next.has(tagId)) next.delete(tagId);
      else next.add(tagId);
      return next;
    });
  };

  // ── Calendar actions ─────────────────────────────────────────────────────

  const prevMonth = () => {
    if (month === 0) { setYear((y) => y - 1); setMonth(11); }
    else setMonth((m) => m - 1);
  };
  const nextMonth = () => {
    if (month === 11) { setYear((y) => y + 1); setMonth(0); }
    else setMonth((m) => m + 1);
  };

  const calOverrides = calUserId !== null
    ? (plan.user_calendar_overrides[calUserId]?.entries ?? {})
    : plan.calendar.entries;

  const firstDay = new Date(year, month, 1);
  const totalDays = daysInMonth(year, month);
  const startOffset = weekdayMon(firstDay);
  const cells: (number | null)[] = [];
  for (let i = 0; i < startOffset; i++) cells.push(null);
  for (let d = 1; d <= totalDays; d++) cells.push(d);
  while (cells.length % 7 !== 0) cells.push(null);

  const cellDate = (day: number): IsoDate => formatDate(new Date(year, month, day));

  const defaultHours = (day: number): number => {
    const d = new Date(year, month, day);
    const wd = d.getDay();
    const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const schedule =
      calUserId !== null
        ? (plan.users_data[calUserId]?.schedule ?? plan.default_schedule)
        : plan.default_schedule;
    return schedule.days[dayNames[wd] as keyof typeof schedule.days] ?? 0;
  };

  const openCalEdit = (day: number) => {
    const iso = cellDate(day);
    const current = calOverrides[iso] ?? null;
    setEditPopup({ date: iso, current, value: current !== null ? String(current) : "" });
  };

  const commitEdit = async () => {
    if (!editPopup) return;
    const hours = parseFloat(editPopup.value);
    if (!isNaN(hours)) {
      if (calUserId !== null) {
        await sendRequest({ SetUserCalendarOverride: [calUserId, editPopup.date, hours] });
      } else {
        await sendRequest({ SetCalendarOverride: [editPopup.date, hours] });
      }
    }
    setEditPopup(null);
  };

  const clearEdit = async () => {
    if (!editPopup) return;
    if (calUserId !== null) {
      await sendRequest({ ClearUserCalendarOverride: [calUserId, editPopup.date] });
    } else {
      await sendRequest({ ClearCalendarOverride: editPopup.date });
    }
    setEditPopup(null);
  };

  const todayIso = formatDate(today);
  const monthName = firstDay.toLocaleString("default", { month: "long" });

  // ── Tag actions ───────────────────────────────────────────────────────────

  const handleAddTag = async () => {
    const name = newTagName.trim();
    if (!name) return;
    await sendRequest({ AddTag: name });
    setNewTagName("");
  };

  const handleRenameTag = async (id: TagId) => {
    const name = (renaming[id] ?? "").trim();
    if (!name) return;
    await sendRequest({ RenameTag: [id, name] });
    setRenaming((r) => { const n = { ...r }; delete n[id]; return n; });
  };

  const handleDeleteTag = async (id: TagId) => {
    await sendRequest({ DeleteTag: id });
  };

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="resources-page">
      {/* ── Left: User management ── */}
      <div className="resources-left">
        <div className="resources-section-header">Users</div>

        {userView.type === "list" && (
          <>
            <div className="resources-user-list">
              {users.length === 0 && (
                <div className="resources-empty">No users yet</div>
              )}
              {users.map((u) => (
                <div
                  key={u.id}
                  className={`resources-user-item${selectedUserId === u.id ? " resources-user-item--selected" : ""}`}
                  onClick={() => setSelectedUserId(u.id === selectedUserId ? null : u.id)}
                >
                  <span className="resources-user-name">{u.name}</span>
                  {u.tags.length > 0 && (
                    <span className="resources-user-tags">
                      {u.tags.map((tid) => plan.tags.find((t) => t.id === tid)?.name).filter(Boolean).join(", ")}
                    </span>
                  )}
                  <button
                    className="resources-user-edit-btn"
                    onClick={(e) => { e.stopPropagation(); openEdit(u); }}
                    title="Edit user"
                  >
                    ✎
                  </button>
                </div>
              ))}
            </div>
            <button className="resources-add-bar" onClick={openCreate}>
              + Add User
            </button>
          </>
        )}

        {(userView.type === "create" || userView.type === "edit") && (
          <div className="resources-user-form">
            <div className="resources-form-title">
              {userView.type === "create" ? "New User" : "Edit User"}
            </div>
            {userError && <div className="resources-form-error">{userError}</div>}
            <label className="resources-form-label">Name</label>
            <input
              className="resources-form-input"
              type="text"
              value={userName}
              autoFocus
              onChange={(e) => setUserName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleSaveUser(); if (e.key === "Escape") setUserView({ type: "list" }); }}
            />
            {plan.tags.length > 0 && (
              <>
                <label className="resources-form-label">Tags</label>
                <div className="resources-tag-chips">
                  {plan.tags.map((tag) => {
                    const on = userTags.has(tag.id);
                    return (
                      <button
                        key={tag.id}
                        className={`resources-tag-chip${on ? " resources-tag-chip--on" : ""}`}
                        onClick={() => toggleUserTag(tag.id)}
                      >
                        {tag.name}
                      </button>
                    );
                  })}
                </div>
              </>
            )}
            <div className="resources-form-actions">
              {userView.type === "edit" && (
                <button
                  className="btn btn-danger btn-sm"
                  onClick={() => handleDeleteUser((userView as { type: "edit"; userId: UserId }).userId)}
                  disabled={userSaving}
                >
                  Delete
                </button>
              )}
              <button className="btn btn-secondary btn-sm" onClick={() => setUserView({ type: "list" })}>
                Cancel
              </button>
              <button
                className="btn btn-primary btn-sm"
                onClick={handleSaveUser}
                disabled={userSaving || !userName.trim()}
              >
                {userSaving ? "Saving…" : "Save"}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* ── Right: Calendar (top) + Tags (bottom) ── */}
      <div className="resources-right">
        {/* Calendar */}
        <div className="resources-calendar">
          <div className="resources-section-header">Calendar Overrides</div>

          {/* User selector tabs */}
          <div className="resources-cal-tabs">
            <button
              className={`resources-cal-tab${calUserId === null ? " resources-cal-tab--active" : ""}`}
              onClick={() => setCalUserId(null)}
            >
              Plan
            </button>
            {users.map((u) => (
              <button
                key={u.id}
                className={`resources-cal-tab${calUserId === u.id ? " resources-cal-tab--active" : ""}`}
                onClick={() => setCalUserId(u.id)}
              >
                {u.name}
              </button>
            ))}
          </div>

          {/* Month navigation */}
          <div className="resources-cal-nav">
            <button className="resources-cal-nav-btn" onClick={prevMonth}>◄</button>
            <span className="resources-cal-month">{monthName} {year}</span>
            <button className="resources-cal-nav-btn" onClick={nextMonth}>►</button>
          </div>

          {/* Grid */}
          <div className="resources-cal-grid">
            {WEEKDAYS.map((wd) => (
              <div key={wd} className="resources-cal-cell resources-cal-cell--header">{wd}</div>
            ))}
            {cells.map((day, i) => {
              if (day === null) return <div key={`pad-${i}`} className="resources-cal-cell resources-cal-cell--empty" />;
              const iso = cellDate(day);
              const override = calOverrides[iso];
              const def = defaultHours(day);
              const isToday = iso === todayIso;
              return (
                <div
                  key={iso}
                  className={`resources-cal-cell resources-cal-cell--day${isToday ? " resources-cal-cell--today" : ""}${override !== undefined ? " resources-cal-cell--overridden" : ""}`}
                  onClick={() => openCalEdit(day)}
                >
                  <span className="resources-cal-day-num">{day}</span>
                  {override !== undefined ? (
                    <span className="resources-cal-day-hours resources-cal-day-hours--override">{override}h</span>
                  ) : def > 0 ? (
                    <span className="resources-cal-day-hours">{def}h</span>
                  ) : null}
                </div>
              );
            })}
          </div>

          {/* Edit popup */}
          {editPopup && (
            <div className="resources-cal-popup-backdrop" onClick={() => setEditPopup(null)}>
              <div className="resources-cal-popup" onClick={(e) => e.stopPropagation()}>
                <div className="resources-cal-popup-date">{editPopup.date}</div>
                <input
                  className="resources-cal-popup-input"
                  type="number"
                  min={0}
                  max={24}
                  step={0.5}
                  value={editPopup.value}
                  autoFocus
                  onChange={(e) => setEditPopup({ ...editPopup, value: e.target.value })}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitEdit();
                    if (e.key === "Escape") setEditPopup(null);
                  }}
                />
                <span className="resources-cal-popup-unit">hours</span>
                <div className="resources-cal-popup-actions">
                  <button className="btn btn-secondary btn-sm" onClick={clearEdit}>Clear</button>
                  <button className="btn btn-primary btn-sm" onClick={commitEdit}>OK</button>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Tags */}
        <div className="resources-tags">
          <div className="resources-section-header">Tags</div>
          <div className="resources-tags-list">
            {plan.tags.length === 0 && (
              <div className="resources-empty">No tags yet</div>
            )}
            {plan.tags.map((tag, idx) => {
              const editVal = renaming[tag.id] ?? tag.name;
              return (
                <div key={tag.id} className="resources-tag-row">
                  <span className="resources-tag-num">{idx + 1}.</span>
                  <input
                    type="text"
                    className="resources-tag-input"
                    value={editVal}
                    onChange={(e) => setRenaming((r) => ({ ...r, [tag.id]: e.target.value }))}
                    onBlur={() => handleRenameTag(tag.id)}
                    onKeyDown={(e) => { if (e.key === "Enter") handleRenameTag(tag.id); }}
                  />
                  <button className="btn btn-danger btn-sm" onClick={() => handleDeleteTag(tag.id)}>×</button>
                </div>
              );
            })}
          </div>
          <div className="resources-tags-add">
            <input
              type="text"
              className="resources-tag-input"
              placeholder="New tag name…"
              value={newTagName}
              onChange={(e) => setNewTagName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleAddTag(); }}
            />
            <button className="btn btn-primary btn-sm" onClick={handleAddTag} disabled={!newTagName.trim()}>
              Add
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
