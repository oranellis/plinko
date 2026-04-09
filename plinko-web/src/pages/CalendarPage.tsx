import { useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { IsoDate, UserId } from "../protocol";
import { formatDate, parseDate } from "../utils/planUtils";
import "./CalendarPage.css";

const WEEKDAYS = ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"];

function firstDayOfMonth(year: number, month: number): Date {
  return new Date(year, month, 1);
}

function daysInMonth(year: number, month: number): number {
  return new Date(year, month + 1, 0).getDate();
}

/** 0-indexed weekday with Monday=0 */
function weekdayMon(d: Date): number {
  return (d.getDay() + 6) % 7;
}

interface EditPopup {
  date: IsoDate;
  current: number | null;
  value: string;
}

export function CalendarPage() {
  const { plan, sendRequest } = usePlanContext();
  const today = new Date();
  const [year, setYear] = useState(today.getFullYear());
  const [month, setMonth] = useState(today.getMonth()); // 0-indexed
  const [selectedUserId, setSelectedUserId] = useState<UserId | null>(null);
  const [editPopup, setEditPopup] = useState<EditPopup | null>(null);

  if (!plan) {
    return (
      <div className="calendar-page calendar-page--empty">
        No plan loaded
      </div>
    );
  }

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  const overrides =
    selectedUserId !== null
      ? (plan.user_calendar_overrides[selectedUserId]?.entries ?? {})
      : plan.calendar.entries;

  const prevMonth = () => {
    if (month === 0) { setYear((y) => y - 1); setMonth(11); }
    else setMonth((m) => m - 1);
  };
  const nextMonth = () => {
    if (month === 11) { setYear((y) => y + 1); setMonth(0); }
    else setMonth((m) => m + 1);
  };

  // Build grid cells
  const firstDay = firstDayOfMonth(year, month);
  const totalDays = daysInMonth(year, month);
  const startOffset = weekdayMon(firstDay);

  const cells: (number | null)[] = [];
  for (let i = 0; i < startOffset; i++) cells.push(null);
  for (let d = 1; d <= totalDays; d++) cells.push(d);
  // Pad to multiple of 7
  while (cells.length % 7 !== 0) cells.push(null);

  const cellDate = (day: number): IsoDate => {
    const d = new Date(year, month, day);
    return formatDate(d);
  };

  const defaultHours = (day: number): number => {
    const d = new Date(year, month, day);
    const wd = d.getDay(); // 0=Sun
    const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const schedule =
      selectedUserId !== null
        ? (plan.users_data[selectedUserId]?.schedule ?? plan.default_schedule)
        : plan.default_schedule;
    return schedule.days[dayNames[wd] as keyof typeof schedule.days] ?? 0;
  };

  const openEdit = (day: number) => {
    const iso = cellDate(day);
    const current = overrides[iso] ?? null;
    setEditPopup({ date: iso, current, value: current !== null ? String(current) : "" });
  };

  const commitEdit = async () => {
    if (!editPopup) return;
    const hours = parseFloat(editPopup.value);
    if (!isNaN(hours)) {
      if (selectedUserId !== null) {
        await sendRequest({ SetUserCalendarOverride: [selectedUserId, editPopup.date, hours] });
      } else {
        await sendRequest({ SetCalendarOverride: [editPopup.date, hours] });
      }
    }
    setEditPopup(null);
  };

  const clearEdit = async () => {
    if (!editPopup) return;
    if (selectedUserId !== null) {
      await sendRequest({ ClearUserCalendarOverride: [selectedUserId, editPopup.date] });
    } else {
      await sendRequest({ ClearCalendarOverride: editPopup.date });
    }
    setEditPopup(null);
  };

  const monthName = firstDay.toLocaleString("default", { month: "long" });
  const todayIso = formatDate(today);

  return (
    <div className="calendar-page">
      {/* User tabs */}
      <div className="calendar-tabs">
        <button
          className={`calendar-tab ${selectedUserId === null ? "calendar-tab--active" : ""}`}
          onClick={() => setSelectedUserId(null)}
        >
          Plan
        </button>
        {users.map((u) => (
          <button
            key={u.id}
            className={`calendar-tab ${selectedUserId === u.id ? "calendar-tab--active" : ""}`}
            onClick={() => setSelectedUserId(u.id)}
          >
            {u.name}
          </button>
        ))}
      </div>

      {/* Month navigation */}
      <div className="calendar-nav">
        <button className="calendar-nav-btn" onClick={prevMonth}>◄</button>
        <span className="calendar-month-label">
          {monthName} {year}
        </span>
        <button className="calendar-nav-btn" onClick={nextMonth}>►</button>
      </div>

      {/* Grid */}
      <div className="calendar-grid">
        {WEEKDAYS.map((wd) => (
          <div key={wd} className="calendar-cell calendar-cell--header">
            {wd}
          </div>
        ))}
        {cells.map((day, i) => {
          if (day === null) return <div key={`pad-${i}`} className="calendar-cell calendar-cell--empty" />;
          const iso = cellDate(day);
          const override = overrides[iso];
          const def = defaultHours(day);
          const isToday = iso === todayIso;
          return (
            <div
              key={iso}
              className={`calendar-cell calendar-cell--day ${isToday ? "calendar-cell--today" : ""} ${override !== undefined ? "calendar-cell--overridden" : ""}`}
              onClick={() => openEdit(day)}
            >
              <span className="calendar-day-num">{day}</span>
              {override !== undefined ? (
                <span className="calendar-day-hours calendar-day-hours--override">{override}h</span>
              ) : def > 0 ? (
                <span className="calendar-day-hours">{def}h</span>
              ) : null}
            </div>
          );
        })}
      </div>

      {/* Edit popup */}
      {editPopup && (
        <div className="calendar-popup-backdrop" onClick={() => setEditPopup(null)}>
          <div className="calendar-popup" onClick={(e) => e.stopPropagation()}>
            <div className="calendar-popup-date">{editPopup.date}</div>
            <input
              className="calendar-popup-input"
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
            <span className="calendar-popup-unit">hours</span>
            <div className="calendar-popup-actions">
              <button className="btn btn-secondary btn-sm" onClick={clearEdit}>
                Clear
              </button>
              <button className="btn btn-primary btn-sm" onClick={commitEdit}>
                OK
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
