import { useCallback, useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import type { TaskId, UserId, WorkSegment } from "../protocol";
import {
  STATUS_COLORS,
  addDays,
  daysBetween,
  displayName,
  formatDate,
  parseDate,
  workerUserId,
} from "../utils/planUtils";
import { TaskFormModal } from "../components/modals/TaskFormModal";
import { UsersModal } from "../components/modals/UsersModal";
import { PlanSettingsModal } from "../components/modals/PlanSettingsModal";
import "./AllocationPage.css";

const USER_PANEL_W = 220;
const LABEL_COL_W = 200;
const ROW_H = 32;
const HEADER_H = 28;
const DAY_W_DEFAULT = 28;
const MIN_DAY_W = 8;
const MAX_DAY_W = 80;

export function AllocationPage() {
  const { plan, sendRequest } = usePlanContext();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [selectedUserId, setSelectedUserId] = useState<UserId | null>(null);
  const [userScrollY, setUserScrollY] = useState(0);
  const [taskScrollY, setTaskScrollY] = useState(0);
  const [scrollX, setScrollX] = useState(0);
  const [dayW, setDayW] = useState(DAY_W_DEFAULT);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const [editTaskId, setEditTaskId] = useState<TaskId | null>(null);
  const [showUsers, setShowUsers] = useState(false);
  const [showSettings, setShowSettings] = useState(false);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver((entries) => {
      const e = entries[0];
      setSize({ w: e.contentRect.width, h: e.contentRect.height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (!plan) return;
    const today = formatDate(new Date());
    const offset = daysBetween(plan.start_date, today);
    const timelineW = size.w - USER_PANEL_W - LABEL_COL_W;
    setScrollX(Math.max(0, offset * dayW - timelineW / 2));
  }, [plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const users = plan
    ? Object.values(plan.users_data)
        .map((ud) => ud.user)
        .sort((a, b) => a.name.localeCompare(b.name))
    : [];

  // Tasks for selected user
  const userTasks = selectedUserId && plan
    ? Object.entries(plan.tasks)
        .filter(([, t]) => t.workers.some((w) => workerUserId(w) === selectedUserId))
        .sort(([, a], [, b]) => a.name.localeCompare(b.name))
    : [];

  // Per-user utilisation summary
  const utilisation = (userId: UserId): number => {
    if (!plan) return 0;
    let total = 0, work = 0;
    for (const [, state] of Object.entries(plan.node_allocations.tasks)) {
      const alloc = "Fixed" in state.allocation ? state.allocation.Fixed : "Dynamic" in state.allocation ? state.allocation.Dynamic : null;
      if (!alloc) continue;
      for (const seg of alloc.time_allocation) {
        if (seg.user === userId) {
          work += seg.hours_worked;
          total += 8; // assume 8h capacity per working day
        }
      }
    }
    return total > 0 ? work / total : 0;
  };

  const hitUserRectsRef = useRef<{ id: string; y: number; h: number }[]>([]);
  const hitTaskRectsRef = useRef<{ id: string; y: number; h: number }[]>([]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !plan) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { w, h } = size;
    canvas.width = w;
    canvas.height = h;
    hitUserRectsRef.current = [];
    hitTaskRectsRef.current = [];

    ctx.fillStyle = "#1e1e1e";
    ctx.fillRect(0, 0, w, h);

    const today = formatDate(new Date());

    // === USER PANEL ===
    ctx.fillStyle = "#252526";
    ctx.fillRect(0, 0, USER_PANEL_W, h);
    ctx.strokeStyle = "#3a3a3c";
    ctx.beginPath(); ctx.moveTo(USER_PANEL_W, 0); ctx.lineTo(USER_PANEL_W, h); ctx.stroke();

    ctx.save();
    ctx.beginPath();
    ctx.rect(0, HEADER_H, USER_PANEL_W, h - HEADER_H);
    ctx.clip();

    for (let i = 0; i < users.length; i++) {
      const u = users[i];
      const y = HEADER_H + i * ROW_H - userScrollY;
      if (y + ROW_H < HEADER_H || y > h) continue;

      const isSelected = u.id === selectedUserId;
      ctx.fillStyle = isSelected ? "#2d4a6a" : (i % 2 === 0 ? "#252526" : "#222224");
      ctx.fillRect(0, y, USER_PANEL_W, ROW_H);

      ctx.fillStyle = "#d4d4d4";
      ctx.font = "13px sans-serif";
      ctx.fillText(u.name, 12, y + ROW_H / 2 + 4);

      // Util bar
      const util = utilisation(u.id);
      const barW = 60;
      const barX = USER_PANEL_W - barW - 8;
      const barY = y + ROW_H / 2 - 4;
      ctx.fillStyle = "#333";
      ctx.fillRect(barX, barY, barW, 8);
      const color = util < 0.8 ? "#4caf50" : util < 1 ? "#ff9800" : "#e53935";
      ctx.fillStyle = color;
      ctx.fillRect(barX, barY, Math.min(barW, util * barW), 8);

      hitUserRectsRef.current.push({ id: u.id, y, h: ROW_H });
    }
    ctx.restore();

    // === LABEL COLUMN ===
    const labelX = USER_PANEL_W;
    ctx.fillStyle = "#252526";
    ctx.fillRect(labelX, 0, LABEL_COL_W, h);
    ctx.strokeStyle = "#3a3a3c";
    ctx.beginPath(); ctx.moveTo(labelX + LABEL_COL_W, 0); ctx.lineTo(labelX + LABEL_COL_W, h); ctx.stroke();

    ctx.save();
    ctx.beginPath();
    ctx.rect(labelX, HEADER_H, LABEL_COL_W, h - HEADER_H);
    ctx.clip();

    for (let i = 0; i < userTasks.length; i++) {
      const [id, task] = userTasks[i];
      const y = HEADER_H + i * ROW_H - taskScrollY;
      if (y + ROW_H < HEADER_H || y > h) continue;

      ctx.fillStyle = i % 2 === 0 ? "#252526" : "#222224";
      ctx.fillRect(labelX, y, LABEL_COL_W, ROW_H);

      const label = displayName(task.name, task.context_label);
      ctx.fillStyle = "#d4d4d4";
      ctx.font = "12px sans-serif";
      ctx.save();
      ctx.beginPath();
      ctx.rect(labelX + 8, y, LABEL_COL_W - 16, ROW_H);
      ctx.clip();
      ctx.fillText(label, labelX + 10, y + ROW_H / 2 + 4);
      ctx.restore();

      hitTaskRectsRef.current.push({ id, y, h: ROW_H });
    }
    ctx.restore();

    // === TIMELINE ===
    const tlX = labelX + LABEL_COL_W;
    const tlW = w - tlX;

    if (tlW <= 0) return;

    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, 0, tlW, h);
    ctx.clip();

    // Today column
    const todayOffset = daysBetween(plan.start_date, today);
    const todayCanvasX = tlX + todayOffset * dayW - scrollX;
    if (todayCanvasX >= tlX && todayCanvasX < w) {
      ctx.fillStyle = "rgba(64,91,200,0.18)";
      ctx.fillRect(todayCanvasX, 0, dayW, h);
    }

    // Day header
    ctx.fillStyle = "#252526";
    ctx.fillRect(tlX, 0, tlW, HEADER_H);
    ctx.strokeStyle = "#3a3a3c";
    ctx.beginPath(); ctx.moveTo(tlX, HEADER_H); ctx.lineTo(w, HEADER_H); ctx.stroke();

    const firstDay = Math.floor(scrollX / dayW);
    const lastDay = Math.ceil((scrollX + tlW) / dayW);

    ctx.font = "10px sans-serif";
    for (let d = firstDay; d <= lastDay; d++) {
      const date = parseDate(addDays(plan.start_date, d));
      const x = tlX + d * dayW - scrollX;
      const dayNum = date.getDate();
      ctx.fillStyle = d === todayOffset ? "#4a90d9" : "#888";
      ctx.textAlign = "center";
      ctx.fillText(String(dayNum), x + dayW / 2, 18);
      ctx.textAlign = "left";
    }

    // Task allocation bars
    for (let i = 0; i < userTasks.length; i++) {
      const [id] = userTasks[i];
      const state = plan.node_allocations.tasks[id as TaskId];
      if (!state) continue;
      const alloc = "Fixed" in state.allocation ? state.allocation.Fixed : "Dynamic" in state.allocation ? state.allocation.Dynamic : null;
      if (!alloc) continue;

      const y = HEADER_H + i * ROW_H - taskScrollY;
      if (y + ROW_H < HEADER_H || y > h) continue;

      // Group consecutive segments into day ranges
      const segsByDate: Record<string, WorkSegment[]> = {};
      for (const seg of alloc.time_allocation) {
        if (seg.user === selectedUserId) {
          (segsByDate[seg.date] ??= []).push(seg);
        }
      }

      const color = STATUS_COLORS[state.status];
      for (const [dateStr, segs] of Object.entries(segsByDate)) {
        const off = daysBetween(plan.start_date, dateStr);
        const x = tlX + off * dayW - scrollX;
        const hours = segs.reduce((s, sg) => s + sg.hours_worked, 0);
        const fill = Math.min(hours / 8, 1);
        const bh = Math.round((ROW_H - 6) * fill);
        ctx.fillStyle = color + "aa";
        ctx.fillRect(x + 1, y + ROW_H - 3 - bh, dayW - 2, bh);
      }
    }

    ctx.restore();
  }, [plan, users, userTasks, size, scrollX, taskScrollY, userScrollY, dayW, selectedUserId]);

  useEffect(() => { render(); }, [render]);

  const onMouseDown = (e: React.MouseEvent) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;

    // User panel click
    if (mx < USER_PANEL_W) {
      for (const r of hitUserRectsRef.current) {
        if (my >= r.y && my <= r.y + r.h) {
          setSelectedUserId((prev) => (prev === r.id ? null : r.id));
          setTaskScrollY(0);
          return;
        }
      }
    }

    // Label column click
    if (mx >= USER_PANEL_W && mx < USER_PANEL_W + LABEL_COL_W) {
      for (const r of hitTaskRectsRef.current) {
        if (my >= r.y && my <= r.y + r.h) {
          setEditTaskId(r.id);
          return;
        }
      }
    }
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    if (mx < USER_PANEL_W) {
      setUserScrollY((sy) => Math.max(0, sy + e.deltaY));
    } else if (mx < USER_PANEL_W + LABEL_COL_W) {
      setTaskScrollY((sy) => Math.max(0, sy + e.deltaY));
    } else {
      if (e.shiftKey) {
        const factor = e.deltaY > 0 ? 0.9 : 1.1;
        setDayW((w) => Math.max(MIN_DAY_W, Math.min(MAX_DAY_W, w * factor)));
      } else {
        setScrollX((sx) => Math.max(0, sx + e.deltaX));
      }
    }
  };
  const onWheelRef = useRef(onWheel);
  onWheelRef.current = onWheel;

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const handler = (e: WheelEvent) => onWheelRef.current(e);
    canvas.addEventListener("wheel", handler, { passive: false });
    return () => canvas.removeEventListener("wheel", handler);
  }, []);

  return (
    <div className="allocation-page" ref={containerRef}>
      {/* Toolbar */}
      <div className="allocation-toolbar">
        <button className="overview-tool-btn" onClick={() => {
          if (!plan) return;
          const today = formatDate(new Date());
          const offset = daysBetween(plan.start_date, today);
          const tlW = size.w - USER_PANEL_W - LABEL_COL_W;
          setScrollX(Math.max(0, offset * dayW - tlW / 2));
        }}>📅 Today</button>
        <span style={{ flex: 1 }} />
        <button className="overview-tool-btn" onClick={() => setShowUsers(true)}>👤 Users</button>
        <button className="overview-tool-btn" onClick={() => setShowSettings(true)}>⚙ Settings</button>
      </div>

      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block" }}
        onMouseDown={onMouseDown}
      />

      {editTaskId && plan?.tasks[editTaskId] && (
        <TaskFormModal
          task={plan.tasks[editTaskId]}
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setEditTaskId(null)}
        />
      )}
      {showUsers && plan && (
        <UsersModal plan={plan} sendRequest={sendRequest} onClose={() => setShowUsers(false)} />
      )}
      {showSettings && plan && (
        <PlanSettingsModal plan={plan} sendRequest={sendRequest} onClose={() => setShowSettings(false)} />
      )}
    </div>
  );
}
