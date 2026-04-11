import { useEffect, useRef, useState } from "react";
import { v4 as uuidv4 } from "uuid";
import { usePlanContext } from "../context/PlanContext";
import type { IsoDate, TagId, User, UserId, Weekday, WorkSchedule } from "../protocol";
import { formatDate } from "../utils/planUtils";
import "./ResourcesPage.css";

// ── Constants ────────────────────────────────────────────────────────────────

const WEEKDAYS: Weekday[] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const WEEKDAYS_SHORT = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

// Number of months to render before/after today in the scrollable calendar
const MONTHS_BEFORE = 6;
const MONTHS_AFTER = 18;

function weekdayMon(d: Date): number {
  return (d.getDay() + 6) % 7;
}
function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

// ── Types ────────────────────────────────────────────────────────────────────

type LeftSelection = "plan" | UserId;
type LeftView = "list" | "create" | { edit: UserId };

// ── ResourcesPage ────────────────────────────────────────────────────────────

export function ResourcesPage() {
  const { plan, sendRequest } = usePlanContext();

  // ── Left panel state ──
  const [selected, setSelected] = useState<LeftSelection>("plan");
  const [leftView, setLeftView] = useState<LeftView>("list");
  const [userName, setUserName] = useState("");
  const [userTags, setUserTags] = useState<Set<TagId>>(new Set());
  const [userSaving, setUserSaving] = useState(false);
  const [userError, setUserError] = useState<string | null>(null);

  // Tags
  const [newTagName, setNewTagName] = useState("");
  const [renaming, setRenaming] = useState<Record<TagId, string>>({});

  // Schedule (shared for plan and user)
  const [scheduleHours, setScheduleHours] = useState<Partial<Record<Weekday, string>>>({});
  const [scheduleSaving, setScheduleSaving] = useState(false);

  // ── Calendar popup ──
  const [editPopup, setEditPopup] = useState<{ date: IsoDate; value: string } | null>(null);

  // Scrollable calendar ref — scroll to today on mount
  const calScrollRef = useRef<HTMLDivElement>(null);
  const todayMarkerRef = useRef<HTMLDivElement>(null);

  if (!plan) {
    return <div className="resources-page resources-page--empty">No plan loaded</div>;
  }

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  // ── Derived schedule for left panel ──────────────────────────────────────

  const baseSchedule = (): WorkSchedule => {
    if (selected === "plan") return plan.default_schedule;
    return plan.users_data[selected]?.schedule ?? plan.default_schedule;
  };

  // Reset schedule editor whenever selection changes
  // (done via useEffect equivalent — we recalculate on demand when rendering)

  // ── User actions ──────────────────────────────────────────────────────────

  const openCreate = () => {
    setUserName("");
    setUserTags(new Set());
    setUserError(null);
    setLeftView("create");
  };

  const openEdit = (user: User) => {
    setUserName(user.name);
    setUserTags(new Set(user.tags));
    setUserError(null);
    setLeftView({ edit: user.id });
  };

  const handleSaveUser = async () => {
    if (!userName.trim()) return;
    setUserSaving(true);
    setUserError(null);
    try {
      const tags = [...userTags];
      if (leftView === "create") {
        const newId = uuidv4();
        const resp = await sendRequest({ CreateUser: { id: newId, name: userName.trim(), tags } });
        if (typeof resp === "object" && "Error" in resp) { setUserError(JSON.stringify(resp.Error)); return; }
        setSelected(newId as UserId);
      } else if (typeof leftView === "object" && "edit" in leftView) {
        const resp = await sendRequest({ UpdateUser: [leftView.edit, { name: userName.trim(), tags }] });
        if (typeof resp === "object" && "Error" in resp) { setUserError(JSON.stringify(resp.Error)); return; }
      }
      setLeftView("list");
    } catch (e) {
      setUserError(String(e));
    } finally {
      setUserSaving(false);
    }
  };

  const handleDeleteUser = async (userId: UserId) => {
    await sendRequest({ DeleteUser: userId });
    if (selected === userId) setSelected("plan");
    setLeftView("list");
  };

  const toggleUserTag = (tagId: TagId) => {
    setUserTags((prev) => {
      const next = new Set(prev);
      if (next.has(tagId)) next.delete(tagId);
      else next.add(tagId);
      return next;
    });
  };

  // ── Schedule actions ──────────────────────────────────────────────────────

  const initScheduleHours = (s: WorkSchedule) =>
    Object.fromEntries(WEEKDAYS.map((wd) => [wd, String(s.days[wd] ?? 0)])) as Record<Weekday, string>;

  const handleSaveSchedule = async () => {
    setScheduleSaving(true);
    try {
      const schedule: WorkSchedule = {
        days: Object.fromEntries(
          WEEKDAYS.map((wd) => [wd, parseFloat(scheduleHours[wd] ?? "0") || 0])
        ) as WorkSchedule["days"],
      };
      if (selected === "plan") {
        await sendRequest({ SetDefaultSchedule: schedule });
      } else {
        await sendRequest({ SetUserSchedule: [selected, schedule] });
      }
    } finally {
      setScheduleSaving(false);
    }
  };

  const handleResetSchedule = async () => {
    if (selected === "plan") return;
    setScheduleSaving(true);
    try {
      await sendRequest({ ClearUserSchedule: selected });
    } finally {
      setScheduleSaving(false);
    }
  };

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

  // ── Calendar helpers ──────────────────────────────────────────────────────

  const calOverrides = selected === "plan"
    ? plan.calendar.entries
    : (plan.user_calendar_overrides[selected]?.entries ?? {});

  const calDefaultHours = (date: Date): number => {
    const wd = date.getDay();
    const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const schedule = selected === "plan"
      ? plan.default_schedule
      : (plan.users_data[selected]?.schedule ?? plan.default_schedule);
    return schedule.days[dayNames[wd] as Weekday] ?? 0;
  };

  const openCalEdit = (iso: IsoDate) => {
    const current = calOverrides[iso] ?? null;
    setEditPopup({ date: iso, value: current !== null ? String(current) : "" });
  };

  const commitEdit = async () => {
    if (!editPopup) return;
    const hours = parseFloat(editPopup.value);
    if (!isNaN(hours)) {
      if (selected === "plan") {
        await sendRequest({ SetCalendarOverride: [editPopup.date, hours] });
      } else {
        await sendRequest({ SetUserCalendarOverride: [selected, editPopup.date, hours] });
      }
    }
    setEditPopup(null);
  };

  const clearEdit = async () => {
    if (!editPopup) return;
    if (selected === "plan") {
      await sendRequest({ ClearCalendarOverride: editPopup.date });
    } else {
      await sendRequest({ ClearUserCalendarOverride: [selected, editPopup.date] });
    }
    setEditPopup(null);
  };

  // ── Build months for the scrollable calendar ──────────────────────────────

  const today = new Date();
  const todayIso = formatDate(today);
  const months: { year: number; month: number }[] = [];
  const startRef = new Date(today.getFullYear(), today.getMonth() - MONTHS_BEFORE, 1);
  for (let i = 0; i < MONTHS_BEFORE + MONTHS_AFTER; i++) {
    months.push({ year: startRef.getFullYear(), month: startRef.getMonth() });
    startRef.setMonth(startRef.getMonth() + 1);
  }

  // ── Schedule section ──────────────────────────────────────────────────────

  const sched = baseSchedule();
  const schedLabel = selected === "plan"
    ? "Plan Default Schedule"
    : `${plan.users_data[selected]?.user.name ?? ""} Schedule`;

  const currentScheduleHours = scheduleHours && Object.keys(scheduleHours).length > 0
    ? scheduleHours
    : initScheduleHours(sched);

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="resources-page">
      {/* ── Left panel ── */}
      <div className="resources-left">

        {leftView === "list" && (
          <>
            {/* ── Users section ── */}
            <div className="resources-section-header">Users</div>
            <div className="resources-user-list">
              {/* Plan row */}
              <div
                className={`resources-user-item${selected === "plan" ? " resources-user-item--selected" : ""}`}
                onClick={() => setSelected("plan")}
              >
                <span className="resources-user-name" style={{ color: "#888", fontStyle: "italic" }}>Plan (everyone)</span>
              </div>

              {users.length === 0 && (
                <div className="resources-empty">No users yet</div>
              )}
              {users.map((u) => (
                <div
                  key={u.id}
                  className={`resources-user-item${selected === u.id ? " resources-user-item--selected" : ""}`}
                  onClick={() => setSelected(u.id)}
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

            {/* Schedule editor for selected item */}
            <div className="resources-schedule-section">
              <div className="resources-section-header resources-section-header--sub">{schedLabel}</div>
              <div className="resources-schedule-grid">
                {WEEKDAYS.map((wd) => (
                  <div key={wd} className="resources-schedule-row">
                    <span className="resources-schedule-day">{wd.slice(0, 3)}</span>
                    <input
                      type="number"
                      min={0}
                      max={24}
                      step={0.5}
                      className="resources-schedule-input"
                      value={currentScheduleHours[wd] ?? "0"}
                      onChange={(e) => setScheduleHours((prev) => ({ ...initScheduleHours(sched), ...prev, [wd]: e.target.value }))}
                    />
                    <span className="resources-schedule-unit">h</span>
                  </div>
                ))}
              </div>
              <div className="resources-schedule-actions">
                {selected !== "plan" && plan.users_data[selected]?.schedule !== null && (
                  <button className="btn btn-secondary btn-sm" onClick={handleResetSchedule} disabled={scheduleSaving}>
                    Reset to default
                  </button>
                )}
                <button className="btn btn-primary btn-sm" onClick={handleSaveSchedule} disabled={scheduleSaving}>
                  {scheduleSaving ? "Saving…" : "Save schedule"}
                </button>
              </div>
            </div>

            {/* ── Tags section ── */}
            <div className="resources-section-header">Tags</div>
            <div className="resources-tags-list">
              {plan.tags.length === 0 && (
                <div className="resources-empty">No tags yet</div>
              )}
              {plan.tags.map((tag, idx) => {
                const editVal = renaming[tag.id] ?? tag.name;
                return (
                  <div key={tag.id} className="resources-user-item resources-tag-row-inline">
                    <span className="resources-tag-num">{idx + 1}.</span>
                    <input
                      type="text"
                      className="resources-tag-input-inline"
                      value={editVal}
                      onChange={(e) => setRenaming((r) => ({ ...r, [tag.id]: e.target.value }))}
                      onBlur={() => handleRenameTag(tag.id)}
                      onKeyDown={(e) => { if (e.key === "Enter") handleRenameTag(tag.id); }}
                    />
                    <button className="resources-user-edit-btn resources-tag-delete" onClick={() => handleDeleteTag(tag.id)} title="Delete tag">
                      ×
                    </button>
                  </div>
                );
              })}
            </div>
            <div className="resources-tags-add-row">
              <input
                type="text"
                className="resources-tag-input-inline resources-tags-add-input"
                placeholder="New tag…"
                value={newTagName}
                onChange={(e) => setNewTagName(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleAddTag(); }}
              />
              <button className="btn btn-primary btn-sm" onClick={handleAddTag} disabled={!newTagName.trim()}>Add</button>
            </div>

            <button className="resources-add-bar" onClick={openCreate}>+ Add User</button>
          </>
        )}

        {(leftView === "create" || typeof leftView === "object") && (
          <div className="resources-user-form">
            <div className="resources-section-header">
              {leftView === "create" ? "New User" : "Edit User"}
            </div>
            {userError && <div className="resources-form-error">{userError}</div>}
            <label className="resources-form-label">Name</label>
            <input
              className="resources-form-input"
              type="text"
              value={userName}
              autoFocus
              onChange={(e) => setUserName(e.target.value)}
              onKeyDown={(e) => { if (e.key === "Enter") handleSaveUser(); if (e.key === "Escape") setLeftView("list"); }}
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
              {typeof leftView === "object" && "edit" in leftView && (
                <button
                  className="btn btn-danger btn-sm"
                  onClick={() => handleDeleteUser(leftView.edit)}
                  disabled={userSaving}
                >
                  Delete
                </button>
              )}
              <button className="btn btn-secondary btn-sm" onClick={() => setLeftView("list")}>Cancel</button>
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

      {/* ── Right: Scrollable calendar ── */}
      <div className="resources-right">
        <div className="resources-cal-label">
          {selected === "plan"
            ? "Plan calendar overrides"
            : `${plan.users_data[selected]?.user.name ?? ""} calendar overrides`}
        </div>

        <div className="resources-calendar-scroll" ref={calScrollRef}>
          {/* Weekday header — sticky */}
          <div className="resources-cal-weekday-header">
            {WEEKDAYS_SHORT.map((wd) => (
              <div key={wd} className="resources-cal-cell resources-cal-cell--header">{wd}</div>
            ))}
          </div>

          {/* Month blocks */}
          {months.map(({ year, month }) => {
            const firstDay = new Date(year, month, 1);
            const totalDays = daysInMonth(year, month);
            const startOffset = weekdayMon(firstDay);
            const cells: (number | null)[] = [];
            for (let i = 0; i < startOffset; i++) cells.push(null);
            for (let d = 1; d <= totalDays; d++) cells.push(d);
            while (cells.length % 7 !== 0) cells.push(null);
            const monthName = firstDay.toLocaleString("default", { month: "long" });
            const isCurrentMonth = year === today.getFullYear() && month === today.getMonth();

            return (
              <div key={`${year}-${month}`} className="resources-month-block">
                <div
                  className={`resources-month-label${isCurrentMonth ? " resources-month-label--current" : ""}`}
                  ref={isCurrentMonth ? todayMarkerRef : undefined}
                >
                  {monthName} {year}
                </div>
                <div className="resources-cal-grid">
                  {cells.map((day, i) => {
                    if (day === null) return <div key={`pad-${i}`} className="resources-cal-cell resources-cal-cell--empty" />;
                    const iso = formatDate(new Date(year, month, day));
                    const override = calOverrides[iso];
                    const def = calDefaultHours(new Date(year, month, day));
                    const isToday = iso === todayIso;
                    return (
                      <div
                        key={iso}
                        className={`resources-cal-cell resources-cal-cell--day${isToday ? " resources-cal-cell--today" : ""}${override !== undefined ? " resources-cal-cell--overridden" : ""}`}
                        onClick={() => openCalEdit(iso)}
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

      {/* Scroll to today on mount */}
      <ScrollToToday markerRef={todayMarkerRef} scrollRef={calScrollRef} />
    </div>
  );
}

// Helper component: scrolls the calendar to today on mount.
function ScrollToToday({
  markerRef,
  scrollRef,
}: {
  markerRef: React.RefObject<HTMLDivElement | null>;
  scrollRef: React.RefObject<HTMLDivElement | null>;
}) {
  useEffect(() => {
    const marker = markerRef.current;
    const scroll = scrollRef.current;
    if (marker && scroll) {
      const top = marker.offsetTop - 60;
      scroll.scrollTop = Math.max(0, top);
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps
  return null;
}


// ── Calendar helpers ─────────────────────────────────────────────────────────

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];
