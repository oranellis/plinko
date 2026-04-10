import { useCallback, useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import type { TaskId, UserId } from "../protocol";
import {
  addDays,
  daysBetween,
  displayName,
  formatDate,
  parseDate,
  workerUserId,
} from "../utils/planUtils";
import { TaskFormModal } from "../components/modals/TaskFormModal";
import { UsersModal } from "../components/modals/UsersModal";
import "./AllocationPage.css";

const USER_PANEL_W = 220;
const LABEL_COL_W = 200;
const ROW_H = 32;
const HEADER_H = 44; // month row (20px) + day row (24px)
const UTIL_ROW_H = 36; // utilisation row below date header
const DAY_W_DEFAULT = 28;
const MIN_DAY_W = 8;
const MAX_DAY_W = 80;

/** Per-index colour palette matching the Rust UI */
const TASK_COLORS = [
  "#4a90d9", "#7ed321", "#f5a623", "#d0021b", "#9b59b6",
  "#1abc9c", "#e67e22", "#2ecc71", "#e74c3c", "#3498db",
];

function taskColorByIndex(idx: number): string {
  return TASK_COLORS[idx % TASK_COLORS.length];
}

function utilColor(frac: number): string {
  if (frac < 0.8) return "#4caf50";
  if (frac <= 1.0) return "#ff9800";
  return "#e53935";
}

/** Get the scheduled start date of a task state */
function allocStartDate(plan: NonNullable<ReturnType<typeof usePlanContext>["plan"]>, taskId: string): string | null {
  const state = plan.node_allocations.tasks[taskId as TaskId];
  if (!state) return null;
  if ("Fixed" in state.allocation) return state.allocation.Fixed.start_date;
  if ("Dynamic" in state.allocation) return state.allocation.Dynamic.scheduled_start_date;
  return null;
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.arcTo(x + w, y, x + w, y + r, r);
  ctx.lineTo(x + w, y + h - r);
  ctx.arcTo(x + w, y + h, x + w - r, y + h, r);
  ctx.lineTo(x + r, y + h);
  ctx.arcTo(x, y + h, x, y + h - r, r);
  ctx.lineTo(x, y + r);
  ctx.arcTo(x, y, x + r, y, r);
  ctx.closePath();
}

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

  const dragRef = useRef({ active: false, startX: 0, lastX: 0, scrollXStart: 0 });

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

  // Centre today on mount / plan load
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

  // Tasks for selected user: exclude Complete/Dropped, sort by start date
  const userTasks: [string, NonNullable<typeof plan>["tasks"][string]][] = selectedUserId && plan
    ? Object.entries(plan.tasks)
        .filter(([id, t]) => {
          // Must be assigned to this user
          if (!t.workers.some((w) => workerUserId(w) === selectedUserId)) return false;
          // Exclude complete and dropped
          const state = plan.node_allocations.tasks[id as TaskId];
          if (state?.status === "Complete" || state?.status === "Dropped") return false;
          return true;
        })
        .sort(([idA], [idB]) => {
          const sa = allocStartDate(plan, idA) ?? "";
          const sb = allocStartDate(plan, idB) ?? "";
          return sa.localeCompare(sb);
        })
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

  // When user selection changes, scroll task list to center on the task closest to starting today
  const sizeRef = useRef(size);
  const dayWRef = useRef(dayW);
  sizeRef.current = size;
  dayWRef.current = dayW;

  useEffect(() => {
    if (!plan || !selectedUserId) { setTaskScrollY(0); return; }
    const today = formatDate(new Date());
    // Find task with start date closest to today
    let bestIdx = 0;
    let bestDiff = Infinity;
    userTasks.forEach(([id], idx) => {
      const sd = allocStartDate(plan, id) ?? plan.start_date;
      const diff = Math.abs(daysBetween(today, sd));
      if (diff < bestDiff) { bestDiff = diff; bestIdx = idx; }
    });
    const contentH = sizeRef.current.h - HEADER_H - UTIL_ROW_H;
    const bestY = bestIdx * ROW_H + ROW_H / 2 - contentH / 2;
    setTaskScrollY(Math.max(0, bestY));
  }, [selectedUserId, plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const hitUserRectsRef = useRef<{ id: string; y: number; h: number }[]>([]);
  const hitTaskRectsRef = useRef<{ id: string; y: number; h: number }[]>([]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !plan) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { w, h } = size;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    ctx.scale(dpr, dpr);
    hitUserRectsRef.current = [];
    hitTaskRectsRef.current = [];

    ctx.fillStyle = "#1e1e1e";
    ctx.fillRect(0, 0, w, h);

    const today = formatDate(new Date());
    const todayOffset = daysBetween(plan.start_date, today);

    // Sorted task IDs for stable colour assignment
    const allTaskIds = Object.keys(plan.tasks).sort((a, b) =>
      (plan.tasks[a as TaskId]?.name ?? "").localeCompare(plan.tasks[b as TaskId]?.name ?? "")
    );
    const colorIndex = (id: string) => allTaskIds.indexOf(id);

    // ── TIMELINE SETUP ────────────────────────────────────────────────────
    const tlX = USER_PANEL_W + LABEL_COL_W;
    const tlW = w - tlX;
    const taskContentTop = HEADER_H + UTIL_ROW_H;

    // ── USER PANEL ────────────────────────────────────────────────────────
    ctx.fillStyle = "#252526";
    ctx.fillRect(0, 0, USER_PANEL_W, h);

    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, USER_PANEL_W, h);
    ctx.clip();

    for (let i = 0; i < users.length; i++) {
      const u = users[i];
      const y = i * ROW_H - userScrollY;
      if (y + ROW_H < 0 || y > h) continue;

      const isSelected = u.id === selectedUserId;
      ctx.fillStyle = isSelected ? "#2d4a6a" : (i % 2 === 0 ? "#252526" : "#222224");
      ctx.fillRect(0, y, USER_PANEL_W, ROW_H);

      // Name (clipped)
      ctx.save();
      ctx.beginPath();
      ctx.rect(10, y, USER_PANEL_W - 80, ROW_H);
      ctx.clip();
      ctx.fillStyle = "#d4d4d4";
      ctx.font = "13px sans-serif";
      ctx.textBaseline = "middle";
      ctx.fillText(u.name, 10, y + ROW_H / 2);
      ctx.textBaseline = "alphabetic";
      ctx.restore();

      // Mini util bar
      const util = utilisation(u.id);
      const barW = 60;
      const barX = USER_PANEL_W - barW - 8;
      const barY = y + ROW_H / 2 - 4;
      ctx.fillStyle = "#333";
      ctx.fillRect(barX, barY, barW, 8);
      ctx.fillStyle = utilColor(util);
      ctx.fillRect(barX, barY, Math.min(barW, util * barW), 8);

      // Pct label
      const pct = `${Math.round(util * 100)}%`;
      ctx.fillStyle = "#888";
      ctx.font = "10px sans-serif";
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      ctx.fillText(pct, USER_PANEL_W - 8, barY + 4);
      ctx.textAlign = "left";
      ctx.textBaseline = "alphabetic";

      // Separator
      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(0, y + ROW_H);
      ctx.lineTo(USER_PANEL_W, y + ROW_H);
      ctx.stroke();

      hitUserRectsRef.current.push({ id: u.id, y, h: ROW_H });
    }
    ctx.restore();

    // User panel right border
    ctx.strokeStyle = "#3a3a3c";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(USER_PANEL_W, 0);
    ctx.lineTo(USER_PANEL_W, h);
    ctx.stroke();

    if (!selectedUserId || tlW <= 0) {
      // Prompt
      if (!selectedUserId) {
        ctx.fillStyle = "#666";
        ctx.font = "14px sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText("Select a user to view their allocation.", (tlX + w) / 2, h / 2);
        ctx.textAlign = "left";
        ctx.textBaseline = "alphabetic";
      }
      return;
    }

    // ── DATE HEADER ───────────────────────────────────────────────────────
    ctx.fillStyle = "#252526";
    ctx.fillRect(tlX, 0, tlW, HEADER_H);

    const firstDay = Math.floor(scrollX / dayW);
    const lastDay = Math.ceil((scrollX + tlW) / dayW);

    // Month row (top 20px)
    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, 0, tlW, HEADER_H);
    ctx.clip();

    let curMonthLabel = "";
    let monthStartX = tlX;
    for (let d = firstDay; d <= lastDay; d++) {
      const date = parseDate(addDays(plan.start_date, d));
      const label = date.toLocaleString("default", { month: "short", year: "numeric" });
      const x = tlX + d * dayW - scrollX;
      if (label !== curMonthLabel) {
        if (curMonthLabel) {
          ctx.fillStyle = "#888";
          ctx.font = "11px sans-serif";
          ctx.textBaseline = "middle";
          ctx.fillText(curMonthLabel, monthStartX + 4, 10);
          ctx.strokeStyle = "#3a3a3c";
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(x, 0);
          ctx.lineTo(x, 20);
          ctx.stroke();
        }
        curMonthLabel = label;
        monthStartX = x;
      }
    }
    if (curMonthLabel) {
      ctx.fillStyle = "#888";
      ctx.font = "11px sans-serif";
      ctx.textBaseline = "middle";
      ctx.fillText(curMonthLabel, monthStartX + 4, 10);
    }
    ctx.textBaseline = "alphabetic";
    ctx.restore();

    // Day row (bottom 24px of header)
    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, 20, tlW, HEADER_H - 20);
    ctx.clip();

    for (let d = firstDay; d <= lastDay; d++) {
      const date = parseDate(addDays(plan.start_date, d));
      const x = tlX + d * dayW - scrollX;
      const isWeekend = date.getDay() === 0 || date.getDay() === 6;
      ctx.fillStyle = isWeekend ? "#1e1e1e" : "#252526";
      ctx.fillRect(x, 20, dayW, HEADER_H - 20);
      ctx.fillStyle = d === todayOffset ? "#4a90d9" : (isWeekend ? "#555" : "#888");
      ctx.font = "11px sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(date.getDate()), x + dayW / 2, 20 + (HEADER_H - 20) / 2);
      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(x, 20);
      ctx.lineTo(x, HEADER_H);
      ctx.stroke();
    }
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
    ctx.restore();

    // Header border
    ctx.strokeStyle = "#3a3a3c";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(tlX, HEADER_H);
    ctx.lineTo(w, HEADER_H);
    ctx.stroke();

    // Today highlight column (behind everything)
    const todayCanvasX = tlX + todayOffset * dayW - scrollX;
    if (todayCanvasX >= tlX && todayCanvasX < w) {
      ctx.fillStyle = "rgba(64,91,200,0.18)";
      ctx.fillRect(todayCanvasX, HEADER_H, dayW, h - HEADER_H);
    }

    // ── UTIL ROW ─────────────────────────────────────────────────────────
    const utilRowTop = HEADER_H;
    const utilRowBottom = utilRowTop + UTIL_ROW_H;

    ctx.fillStyle = "#222224";
    ctx.fillRect(tlX, utilRowTop, tlW, UTIL_ROW_H);

    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, utilRowTop, tlW, UTIL_ROW_H);
    ctx.clip();

    for (let d = firstDay; d <= lastDay; d++) {
      const dateStr = addDays(plan.start_date, d);
      const x = tlX + d * dayW - scrollX;
      // Sum hours for this user on this date across all tasks
      let totalHours = 0;
      for (const state of Object.values(plan.node_allocations.tasks)) {
        const segs = "Fixed" in state.allocation
          ? state.allocation.Fixed.time_allocation
          : state.allocation.Dynamic.time_allocation;
        for (const seg of segs) {
          if (seg.user === selectedUserId && seg.date === dateStr) {
            totalHours += seg.hours_worked;
          }
        }
      }
      if (totalHours > 0) {
        const cap = 8; // hours per day
        const frac = totalHours / cap;
        const barH = Math.max(2, Math.min(frac, 1) * (UTIL_ROW_H - 4));
        const barY = utilRowBottom - 2 - barH;
        const barW = Math.max(1, dayW - 2);
        ctx.fillStyle = utilColor(frac);
        ctx.fillRect(x + 1, barY, barW, barH);

        // Hours label if bar tall enough
        const label = Number.isInteger(totalHours) ? `${totalHours}h` : `${totalHours.toFixed(1)}h`;
        ctx.font = "9px sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const textW = ctx.measureText(label).width;
        if (barH >= 12 && textW < barW - 2) {
          // Backdrop
          const padX = 1, padY = 1;
          ctx.fillStyle = "rgba(0,0,0,0.5)";
          roundRect(ctx, x + 1 + (barW - textW) / 2 - padX, barY + barH / 2 - 6 + padY, textW + padX * 2, 12, 2);
          ctx.fill();
          ctx.fillStyle = "#d4d4d4";
          ctx.fillText(label, x + 1 + barW / 2, barY + barH / 2 + 0.5);
        }
        ctx.textAlign = "left";
        ctx.textBaseline = "alphabetic";
      }
    }
    ctx.restore();

    // Util row border
    ctx.strokeStyle = "#3a3a3c";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(tlX, utilRowBottom);
    ctx.lineTo(w, utilRowBottom);
    ctx.stroke();

    // ── LABEL COLUMN ─────────────────────────────────────────────────────
    const labelX = USER_PANEL_W;
    ctx.fillStyle = "#252526";
    ctx.fillRect(labelX, taskContentTop, LABEL_COL_W, h - taskContentTop);

    ctx.save();
    ctx.beginPath();
    ctx.rect(labelX, taskContentTop, LABEL_COL_W, h - taskContentTop);
    ctx.clip();
    ctx.translate(0, -taskScrollY);

    for (let i = 0; i < userTasks.length; i++) {
      const [id, task] = userTasks[i];
      const rowTop = taskContentTop + i * ROW_H;
      if (rowTop + ROW_H + taskScrollY < taskContentTop || rowTop - taskScrollY > h) continue;

      ctx.fillStyle = i % 2 === 0 ? "#252526" : "#222224";
      ctx.fillRect(labelX, rowTop, LABEL_COL_W, ROW_H);

      const label = displayName(task.name, task.context_label);
      ctx.save();
      ctx.beginPath();
      ctx.rect(labelX + 8, rowTop, LABEL_COL_W - 16, ROW_H);
      ctx.clip();
      ctx.fillStyle = "#d4d4d4";
      ctx.font = "12px sans-serif";
      ctx.textBaseline = "middle";
      ctx.fillText(label, labelX + 10, rowTop + ROW_H / 2);
      ctx.textBaseline = "alphabetic";
      ctx.restore();

      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(labelX, rowTop + ROW_H);
      ctx.lineTo(labelX + LABEL_COL_W, rowTop + ROW_H);
      ctx.stroke();

      hitTaskRectsRef.current.push({ id, y: rowTop - taskScrollY, h: ROW_H });
    }
    ctx.restore();

    // Label column right border
    ctx.strokeStyle = "#3a3a3c";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(labelX + LABEL_COL_W, taskContentTop);
    ctx.lineTo(labelX + LABEL_COL_W, h);
    ctx.stroke();

    // ── TASK BARS ─────────────────────────────────────────────────────────
    if (tlW <= 0) return;

    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, taskContentTop, tlW, h - taskContentTop);
    ctx.clip();
    ctx.translate(0, -taskScrollY);

    for (let i = 0; i < userTasks.length; i++) {
      const [id] = userTasks[i];
      const state = plan.node_allocations.tasks[id as TaskId];
      if (!state) continue;
      const alloc = "Fixed" in state.allocation ? state.allocation.Fixed : state.allocation.Dynamic;
      const rowTop = taskContentTop + i * ROW_H;
      const cidx = colorIndex(id);
      const barColor = taskColorByIndex(cidx);

      // Row background alternating
      ctx.fillStyle = i % 2 === 0 ? "#1e1e1e" : "#202022";
      ctx.fillRect(tlX, rowTop, tlW + scrollX, ROW_H);

      // Per-day bars
      for (const seg of alloc.time_allocation) {
        if (seg.user !== selectedUserId) continue;
        const off = daysBetween(plan.start_date, seg.date);
        const x = tlX + off * dayW - scrollX;
        if (x + dayW < tlX || x > w) continue;

        const cap = 8;
        const frac = Math.min(seg.hours_worked / cap, 1);
        const bh = Math.max(2, frac * (ROW_H - 4));
        const by = rowTop + ROW_H - 2 - bh;
        const bw = Math.max(1, dayW - 2);

        ctx.fillStyle = barColor;
        ctx.fillRect(x + 1, by, bw, bh);

        // Hours label with semi-transparent backdrop
        const label = Number.isInteger(seg.hours_worked) ? `${seg.hours_worked}h` : `${seg.hours_worked.toFixed(1)}h`;
        ctx.font = "9px sans-serif";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const tw = ctx.measureText(label).width;
        const tx = x + 1 + bw / 2;
        const ty = rowTop + ROW_H / 2;
        // Backdrop
        const padX = 2, padY = 1;
        ctx.fillStyle = "rgba(0,0,0,0.5)";
        roundRect(ctx, tx - tw / 2 - padX, ty - 6 + padY, tw + padX * 2, 11, 2);
        ctx.fill();
        ctx.fillStyle = "#bbb";
        ctx.fillText(label, tx, ty + 0.5);
        ctx.textAlign = "left";
        ctx.textBaseline = "alphabetic";
      }

      // Row separator
      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(tlX, rowTop + ROW_H);
      ctx.lineTo(w, rowTop + ROW_H);
      ctx.stroke();
    }

    // Today line
    const todayLineX = tlX + todayOffset * dayW - scrollX;
    if (todayLineX >= tlX && todayLineX <= w) {
      ctx.strokeStyle = "rgba(74,144,217,0.7)";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(todayLineX, taskContentTop - taskScrollY);
      ctx.lineTo(todayLineX, taskContentTop + userTasks.length * ROW_H - taskScrollY);
      ctx.stroke();
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
          return;
        }
      }
      return;
    }

    // Label column click
    if (mx < USER_PANEL_W + LABEL_COL_W) {
      for (const r of hitTaskRectsRef.current) {
        if (my >= r.y && my <= r.y + r.h) {
          setEditTaskId(r.id);
          return;
        }
      }
      return;
    }

    // Timeline drag
    dragRef.current = { active: true, startX: e.clientX, lastX: e.clientX, scrollXStart: scrollX };
  };

  const onMouseMove = (e: React.MouseEvent) => {
    if (!dragRef.current.active) return;
    const dx = e.clientX - dragRef.current.lastX;
    dragRef.current.lastX = e.clientX;
    setScrollX((sx) => Math.max(0, sx - dx));
  };

  const onMouseUp = () => { dragRef.current.active = false; };
  const onMouseLeave = () => { dragRef.current.active = false; };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    if (mx < USER_PANEL_W) {
      const maxU = Math.max(0, users.length * ROW_H - size.h);
      setUserScrollY((sy) => Math.max(0, Math.min(maxU, sy + e.deltaY)));
    } else if (mx < USER_PANEL_W + LABEL_COL_W) {
      const maxT = Math.max(0, userTasks.length * ROW_H - (size.h - HEADER_H - UTIL_ROW_H));
      setTaskScrollY((sy) => Math.max(0, Math.min(maxT, sy + e.deltaY)));
    } else {
      if (e.shiftKey) {
        const factor = e.deltaY > 0 ? 0.9 : 1.1;
        setDayW((dw) => Math.max(MIN_DAY_W, Math.min(MAX_DAY_W, dw * factor)));
      } else if (Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
        const maxT = Math.max(0, userTasks.length * ROW_H - (size.h - HEADER_H - UTIL_ROW_H));
        setTaskScrollY((sy) => Math.max(0, Math.min(maxT, sy + e.deltaY)));
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
      </div>

      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block", cursor: dragRef.current.active ? "grabbing" : "default" }}
        onMouseDown={onMouseDown}
        onMouseMove={onMouseMove}
        onMouseUp={onMouseUp}
        onMouseLeave={onMouseLeave}
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
    </div>
  );
}
