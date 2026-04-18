import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import type { ConstraintViolation, TaskId, UserId } from "../protocol";
import {
  addDays,
  daysBetween,
  displayName,
  formatDate,
  parseDate,
  workerUserId,
} from "../utils/planUtils";
import { TaskFormModal } from "../components/modals/TaskFormModal";
import { IconToday } from "../components/icons";
import "./AllocationPage.css";

const USER_PANEL_W = 220;
const ROW_H = 32;
const HEADER_H = 44; // month row (20px) + day row (24px)
const UTIL_ROW_H = 36; // utilisation row below date header
const DAY_W_DEFAULT = 28;

// Shared Monday logo image (loaded once at module init)
const _mondayLogoImg = new Image();
_mondayLogoImg.src = "/monday_icon.svg";
const MONDAY_LOGO_ASPECT = 110 / 43.261905;

function drawMondayMark(ctx: CanvasRenderingContext2D, rightX: number, centerY: number, size: number): void {
  ctx.save();
  const logoH = size * 1.1;
  const logoW = logoH * MONDAY_LOGO_ASPECT;
  ctx.font = `bold ${size}px sans-serif`;
  const tickW = ctx.measureText("✓").width;
  const tickGap = size * 0.2;
  const logoLeft = rightX - tickW - tickGap - logoW;
  if (_mondayLogoImg.complete && _mondayLogoImg.naturalWidth > 0) {
    ctx.drawImage(_mondayLogoImg, logoLeft, centerY - logoH / 2, logoW, logoH);
  } else {
    const r = size * 0.27;
    const ds = r * 2.6;
    ["#FF3D57", "#FFCB00", "#4EADFD"].forEach((c, i) => {
      ctx.fillStyle = c;
      ctx.beginPath();
      ctx.arc(logoLeft + r + i * ds, centerY, r, 0, Math.PI * 2);
      ctx.fill();
    });
  }
  ctx.fillStyle = "#00C875";
  ctx.textAlign = "right";
  ctx.textBaseline = "middle";
  ctx.fillText("✓", rightX, centerY);
  ctx.restore();
}
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
  const { plan, sendRequest, hasMondayIntegration, setToolbarActions, setToolbarRightActions } = usePlanContext();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  const [selectedUserId, setSelectedUserId] = useState<UserId | null>(null);
  const [userScrollY, setUserScrollY] = useState(0);
  const [taskScrollY, setTaskScrollY] = useState(0);
  const [scrollX, setScrollX] = useState(0);
  const [dayW, setDayW] = useState(DAY_W_DEFAULT);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const [editTaskId, setEditTaskId] = useState<TaskId | null>(null);
  const [showUsers, setShowUsers] = useState(false); // kept for ref compatibility
  const [hoveredTaskId, setHoveredTaskId] = useState<string | null>(null);

  // null = follow auto; toggles to a boolean when user manually overrides
  const [userPanelOverride, setUserPanelOverride] = useState<boolean | null>(null);
  const autoCollapsed = size.w < 600;
  const panelCollapsed = userPanelOverride ?? autoCollapsed;

  // Monday-linked node IDs (set of raw task UUID strings)
  const [mondayLinkedIds, setMondayLinkedIds] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (!hasMondayIntegration || !plan) return;
    sendRequest({ LoadMondayConfig: { plan_id: plan.id } }).then((res) => {
      if (typeof res === "object" && res !== null && "MondayConfigLoaded" in res) {
        const ids = new Set(
          res.MondayConfigLoaded.item_node_map
            .map((m) => {
              const nid = m.plinko_node_id;
              if (nid === "PlanStart") return null;
              if (typeof nid === "object" && "Task" in nid) return nid.Task;
              if (typeof nid === "object" && "Milestone" in nid) return nid.Milestone;
              return null;
            })
            .filter((id): id is string => id !== null)
        );
        setMondayLinkedIds(ids);
      }
    }).catch(() => { /* ignore config load errors silently */ });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasMondayIntegration, plan?.id]);

  // Map from raw UUID string → ConstraintViolation for nodes with violated constraints
  const constraintViolations = useMemo((): Map<string, ConstraintViolation> => {
    if (!plan) return new Map();
    const result = new Map<string, ConstraintViolation>();
    for (const [key, v] of Object.entries(plan.node_allocations.constraint_violations)) {
      if (key.startsWith("task:")) result.set(key.slice(5), v);
      else if (key.startsWith("milestone:")) result.set(key.slice(10), v);
    }
    return result;
  }, [plan]);

  const dragRef = useRef<{
    active: boolean;
    startX: number; lastX: number;
    startY: number; lastY: number;
    scrollXStart: number;
    moved: boolean;
    isTouch: boolean;
    area: 'user' | 'task' | 'header' | null;
    axis: 'h' | 'v' | null;
  }>({ active: false, startX: 0, lastX: 0, startY: 0, lastY: 0, scrollXStart: 0, moved: false, isTouch: false, area: null, axis: null });
  const pendingClickRef = useRef<string | null>(null);
  const mousePosRef = useRef({ x: 0, y: 0 });

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

  // Refs to avoid stale closures in toolbar action callbacks
  const setShowUsersRef = useRef(setShowUsers);
  const planRef = useRef(plan);
  const scrollXRef = useRef(scrollX);
  const sizeRef2 = useRef(size);
  const dayWRef2 = useRef(dayW);
  const panelCollapsedRef = useRef(panelCollapsed);
  setShowUsersRef.current = setShowUsers;
  planRef.current = plan;
  scrollXRef.current = scrollX;
  sizeRef2.current = size;
  dayWRef2.current = dayW;
  panelCollapsedRef.current = panelCollapsed;

  useEffect(() => {
    setToolbarActions(
      <button className="toolbar-btn" title="Jump to today" onClick={() => {
        const p = planRef.current;
        const sw = sizeRef2.current.w;
        const dw = dayWRef2.current;
        if (!p) return;
        const today = formatDate(new Date());
        const offset = daysBetween(p.start_date, today);
        const epw = panelCollapsedRef.current ? 0 : USER_PANEL_W;
        const tlW = sw - epw;
        setScrollX(Math.max(0, offset * dw - tlW / 2));
      }}><IconToday size={24} /></button>
    );
    setToolbarRightActions(null);
    return () => { setToolbarActions(null); setToolbarRightActions(null); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Centre today on mount / plan load
  useEffect(() => {
    if (!plan) return;
    const today = formatDate(new Date());
    const offset = daysBetween(plan.start_date, today);
    const timelineW = size.w - (panelCollapsedRef.current ? 0 : USER_PANEL_W);
    setScrollX(Math.max(0, offset * dayW - timelineW / 2));
  }, [plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const users = useMemo(
    () => plan
      ? Object.values(plan.users_data)
          .map((ud) => ud.user)
          .sort((a, b) => a.name.localeCompare(b.name))
      : [],
    [plan],
  );

  // Tasks for selected user: exclude Complete/Dropped, sort by start date
  const userTasks: [string, NonNullable<typeof plan>["tasks"][string]][] = useMemo(
    () => selectedUserId && plan
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
      : [],
    [plan, selectedUserId],
  );

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
  const hitViolationRectsRef = useRef<{ id: string; x: number; y: number; r: number }[]>([]);

  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !plan) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { w, h } = size;
    const dpr = window.devicePixelRatio || 1;
    const effectivePanelW = panelCollapsed ? 0 : USER_PANEL_W;
    canvas.width = w * dpr;
    canvas.height = h * dpr;
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    ctx.scale(dpr, dpr);
    hitUserRectsRef.current = [];
    hitTaskRectsRef.current = [];
    hitViolationRectsRef.current = [];

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
    const tlX = effectivePanelW;
    const tlW = w - tlX;
    const taskContentTop = HEADER_H + UTIL_ROW_H;

    // ── USER PANEL ────────────────────────────────────────────────────────
    ctx.fillStyle = "#252526";
    ctx.fillRect(0, 0, effectivePanelW, h);

    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, effectivePanelW, h);
    ctx.clip();

    for (let i = 0; i < users.length; i++) {
      const u = users[i];
      const y = i * ROW_H - userScrollY;
      if (y + ROW_H < 0 || y > h) continue;

      const isSelected = u.id === selectedUserId;
      ctx.fillStyle = isSelected ? "#2d4a6a" : (i % 2 === 0 ? "#252526" : "#222224");
      ctx.fillRect(0, y, effectivePanelW, ROW_H);

      // Name (full width)
      ctx.save();
      ctx.beginPath();
      ctx.rect(10, y, effectivePanelW - 16, ROW_H);
      ctx.clip();
      ctx.fillStyle = "#d4d4d4";
      ctx.font = "13px sans-serif";
      ctx.textBaseline = "middle";
      ctx.fillText(u.name, 10, y + ROW_H / 2);
      ctx.textBaseline = "alphabetic";
      ctx.restore();

      // Separator
      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(0, y + ROW_H);
      ctx.lineTo(effectivePanelW, y + ROW_H);
      ctx.stroke();

      hitUserRectsRef.current.push({ id: u.id, y, h: ROW_H });
    }
    ctx.restore();

    // User panel right border
    ctx.strokeStyle = "#3a3a3c";
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(effectivePanelW, 0);
    ctx.lineTo(effectivePanelW, h);
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

    // ── TASK BARS ─────────────────────────────────────────────────────────
    if (tlW <= 0) return;

    ctx.save();
    ctx.beginPath();
    ctx.rect(tlX, taskContentTop, tlW, h - taskContentTop);
    ctx.clip();
    ctx.translate(0, -taskScrollY);

    for (let i = 0; i < userTasks.length; i++) {
      const [id, task] = userTasks[i];
      const state = plan.node_allocations.tasks[id as TaskId];
      if (!state) continue;
      const alloc = "Fixed" in state.allocation ? state.allocation.Fixed : state.allocation.Dynamic;
      const rowTop = taskContentTop + i * ROW_H;
      const cidx = colorIndex(id);
      const barColor = taskColorByIndex(cidx);

      // Row background alternating
      ctx.fillStyle = i % 2 === 0 ? "#1e1e1e" : "#202022";
      ctx.fillRect(tlX, rowTop, tlW + scrollX, ROW_H);

      // Hover highlight
      if (id === hoveredTaskId) {
        ctx.fillStyle = "rgba(255,255,255,0.05)";
        ctx.fillRect(tlX, rowTop, tlW + scrollX, ROW_H);
      }

      // Today column highlight (consistent with header)
      const todayRX = tlX + todayOffset * dayW - scrollX;
      if (todayRX >= tlX && todayRX < tlX + tlW) {
        ctx.fillStyle = "rgba(64,91,200,0.15)";
        ctx.fillRect(todayRX, rowTop, dayW, ROW_H);
      }

      // Per-day bars — track whether any non-zero segments were drawn
      let anyBars = false;
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
        anyBars = true;

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

      // 0h span indicator — show the task's scheduled date range as a thin line
      if (!anyBars) {
        const startDate = "Fixed" in state.allocation
          ? state.allocation.Fixed.start_date
          : state.allocation.Dynamic.scheduled_start_date;
        const durationDays = task.duration_days_target > 0 ? task.duration_days_target : 1;
        if (startDate) {
          const startOff = daysBetween(plan.start_date, startDate);
          const spanX = tlX + startOff * dayW - scrollX;
          const spanW = Math.max(dayW, durationDays * dayW);
          const lineY = rowTop + ROW_H / 2;
          ctx.strokeStyle = barColor + "60";
          ctx.lineWidth = 2;
          ctx.setLineDash([4, 3]);
          ctx.beginPath();
          ctx.moveTo(Math.max(tlX, spanX), lineY);
          ctx.lineTo(Math.min(tlX + tlW + scrollX, spanX + spanW), lineY);
          ctx.stroke();
          ctx.setLineDash([]);

          // Start/end date labels
          const fmt = (d: string) => {
            const dt = new Date(d + "T00:00:00");
            return dt.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
          };
          const endDate = addDays(startDate, Math.ceil(durationDays));
          ctx.font = "9px sans-serif";
          ctx.textAlign = "left";
          ctx.textBaseline = "middle";
          ctx.fillStyle = barColor + "99";
          const lx = Math.max(tlX + 4, spanX + 4);
          const dateLabel = `${fmt(startDate)} – ${fmt(endDate)}`;
          if (lx + 2 < tlX + tlW + scrollX) {
            ctx.fillText(dateLabel, lx, lineY);
          }
          ctx.textBaseline = "alphabetic";
        }
      }

      // Floating task label — drawn over the timeline at the left edge (max ~200px)
      {
        const MAX_LABEL_W = 200;
        let label = displayName(task.name, task.context_label ?? null);
        ctx.font = "13px sans-serif";
        if (ctx.measureText(label).width > MAX_LABEL_W) {
          // Binary search for largest prefix that fits with ellipsis.
          let lo = 0, hi = label.length;
          while (lo < hi - 1) {
            const mid = (lo + hi) >> 1;
            if (ctx.measureText(label.slice(0, mid) + "…").width <= MAX_LABEL_W) lo = mid;
            else hi = mid;
          }
          label = label.slice(0, lo) + "…";
        }
        const tw = ctx.measureText(label).width;
        const padX = 6, padY = 3;
        const lx = tlX + 6;
        const ly = rowTop + ROW_H / 2;
        ctx.fillStyle = "rgba(40,40,40,0.75)";
        roundRect(ctx, lx - padX, ly - 9 - padY, tw + padX * 2, 18 + padY * 2, 3);
        ctx.fill();
        ctx.fillStyle = id === hoveredTaskId ? "#f0f0f0" : "#c8c8c8";
        ctx.textBaseline = "middle";
        ctx.fillText(label, lx, ly);
        ctx.textBaseline = "alphabetic";
      }

      hitTaskRectsRef.current.push({ id, y: rowTop - taskScrollY, h: ROW_H });

      // Row separator
      ctx.strokeStyle = "#2a2a2c";
      ctx.lineWidth = 0.5;
      ctx.beginPath();
      ctx.moveTo(tlX, rowTop + ROW_H);
      ctx.lineTo(w, rowTop + ROW_H);
      ctx.stroke();
    }

    // Vertical day separators (same style as gantt)
    ctx.strokeStyle = "#2a2a2c";
    ctx.lineWidth = 0.5;
    for (let d = firstDay; d <= lastDay; d++) {
      const x = tlX + d * dayW - scrollX;
      if (x < tlX || x > tlX + tlW) continue;
      ctx.beginPath();
      ctx.moveTo(x, taskContentTop);
      ctx.lineTo(x, h);
      ctx.stroke();
    }

    // === HOVER INFO PANEL ===
    if (hoveredTaskId && selectedUserId) {
      const isViolationHover = hoveredTaskId.startsWith("!:");
      const rawId = isViolationHover ? hoveredTaskId.slice(2) : hoveredTaskId;
      const task = plan.tasks[rawId as TaskId];
      const state = plan.node_allocations.tasks[rawId as TaskId];
      if (task && state) {
        const violation = constraintViolations.get(rawId);
        const dpr = window.devicePixelRatio || 1;
        ctx.save();
        ctx.resetTransform();
        ctx.scale(dpr, dpr);

        if (isViolationHover && violation) {
          // Compact floating tooltip near cursor, orange border
          const kindLabel = violation.kind === "Fixed" ? "Fixed date violated"
            : violation.kind === "Latest" ? "Latest date exceeded"
            : "Earliest date not met";
          const lines = [kindLabel, `Required: ${violation.required_date}`, `Scheduled: ${violation.scheduled_date}`];
          const pad = 7;
          const lineH = 17;
          ctx.font = "12px sans-serif";
          const panelW = lines.reduce((mx, l) => Math.max(mx, ctx.measureText(l).width), 0) + pad * 2;
          const panelH = lines.length * lineH + pad * 2 - 3;
          const mxPos = mousePosRef.current.x;
          const myPos = mousePosRef.current.y;
          let px = mxPos - panelW - 14;
          let py = myPos - panelH / 2;
          if (px < effectivePanelW + 4) px = mxPos + 14;
          if (py < 4) py = 4;
          if (py + panelH > h - 4) py = h - 4 - panelH;

          ctx.fillStyle = "rgba(18,18,18,0.95)";
          roundRect(ctx, px, py, panelW, panelH, 4);
          ctx.fill();
          ctx.strokeStyle = "#e65100";
          ctx.lineWidth = 1.5;
          roundRect(ctx, px, py, panelW, panelH, 4);
          ctx.stroke();

          ctx.fillStyle = "#e8a87c";
          ctx.textBaseline = "top";
          for (let i = 0; i < lines.length; i++) {
            ctx.fillText(lines[i], px + pad, py + pad + i * lineH);
          }
          ctx.textBaseline = "alphabetic";
        } else {
          // Standard bottom-left info panel
          const panelPad = 10;
          const margin = 8;
          const lineGap = 4;
          const titleSize = 18;
          const bodySize = 14;

          const lines: string[] = [];
          lines.push(displayName(task.name, task.context_label ?? null));
          const isMonday = mondayLinkedIds.has(rawId);
          lines.push(`Status: ${state.status}`);
          const schedStart = "Fixed" in state.allocation ? state.allocation.Fixed.start_date : state.allocation.Dynamic.scheduled_start_date;
          lines.push(`Scheduled: ${schedStart}`);
          const endDate = "Fixed" in state.allocation ? state.allocation.Fixed.end_date : state.allocation.Dynamic.scheduled_end_date;
          lines.push(`Ends: ${endDate}`);
          const workerNames = task.workers.flatMap((slot) => {
            if ("Specific" in slot) {
              const u = plan.users_data[slot.Specific.user_id];
              return u ? [u.user.name] : [];
            }
            return ["(unassigned)"];
          });
          if (workerNames.length > 0) lines.push(`Workers: ${workerNames.join(", ")}`);
          if (violation) lines.push("⚠ Constraint violated");

          ctx.font = `${titleSize}px sans-serif`;
          const titleW = ctx.measureText(lines[0]).width;
          ctx.font = `${bodySize}px sans-serif`;
          const mondayExtraW = isMonday ? bodySize * (1.1 * MONDAY_LOGO_ASPECT + 0.6 + 0.75) + bodySize * 0.5 : 0;
          let bodyMaxW = 0;
          for (let i = 0; i < lines.length - 1; i++) {
            bodyMaxW = Math.max(bodyMaxW, ctx.measureText(lines[i + 1]).width + (isMonday && i === 0 ? mondayExtraW : 0));
          }
          const panelW = Math.max(titleW, bodyMaxW) + panelPad * 2;
          const panelH = panelPad * 2
            + titleSize * 1.25
            + (lines.length > 1 ? lineGap + (lines.length - 1) * (bodySize * 1.4) + (lines.length - 2) * lineGap : 0);

          const px = effectivePanelW + margin;
          const py = h - margin - panelH;

          ctx.fillStyle = "rgba(0,0,0,0.19)";
          roundRect(ctx, px + 2, py + 3, panelW, panelH, 6);
          ctx.fill();

          ctx.fillStyle = "rgba(30,30,30,0.86)";
          roundRect(ctx, px, py, panelW, panelH, 6);
          ctx.fill();

          ctx.strokeStyle = "#4a4a4a";
          ctx.lineWidth = 1;
          roundRect(ctx, px, py, panelW, panelH, 6);
          ctx.stroke();

          ctx.font = `${titleSize}px sans-serif`;
          ctx.fillStyle = "#d4d4d4";
          ctx.textBaseline = "top";
          ctx.fillText(lines[0], px + panelPad, py + panelPad);

          ctx.font = `${bodySize}px sans-serif`;
          ctx.fillStyle = "#8a8a8a";
          const bodyStartY = py + panelPad + titleSize * 1.25 + lineGap;
          for (let i = 0; i < lines.length - 1; i++) {
            ctx.fillText(lines[i + 1], px + panelPad, bodyStartY + i * (bodySize * 1.4 + lineGap));
          }
          if (isMonday) {
            const markRightX = px + panelW - panelPad;
            const markCenterY = bodyStartY + bodySize * 0.55;
            drawMondayMark(ctx, markRightX, markCenterY, bodySize);
          }
          ctx.textBaseline = "alphabetic";
        }

        ctx.restore();
      }
    }

    // ── CONSTRAINT VIOLATION INDICATORS ──────────────────────────────────
    // Drawn outside the clip region so they're always visible at the right edge
    const excX = w - 12;
    for (let i = 0; i < userTasks.length; i++) {
      const [id] = userTasks[i];
      const violation = constraintViolations.get(id);
      if (!violation) continue;
      const rowCanvasY = taskContentTop + i * ROW_H - taskScrollY;
      if (rowCanvasY + ROW_H < taskContentTop || rowCanvasY > h) continue; // off-screen
      const excY = rowCanvasY + ROW_H / 2;
      ctx.fillStyle = "#e65100";
      ctx.font = "bold 16px sans-serif";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText("!", excX, excY);
      ctx.textAlign = "left";
      ctx.textBaseline = "alphabetic";
      hitViolationRectsRef.current.push({ id, x: excX - 8, y: rowCanvasY, r: 16 });
    }

    ctx.restore();
  }, [plan, users, userTasks, size, scrollX, taskScrollY, userScrollY, dayW, selectedUserId, hoveredTaskId, mondayLinkedIds, constraintViolations, panelCollapsed]);

  useEffect(() => { render(); }, [render]);

  const onPointerDown = (e: React.PointerEvent<HTMLCanvasElement>) => {
    e.currentTarget.setPointerCapture(e.pointerId);
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const epw = panelCollapsed ? 0 : USER_PANEL_W;

    // User panel area (only reachable when panel is visible)
    if (mx < epw) {
      if (e.pointerType === "mouse") {
        for (const r of hitUserRectsRef.current) {
          if (my >= r.y && my <= r.y + r.h) {
            setSelectedUserId((prev) => (prev === r.id ? null : r.id));
            break;
          }
        }
        return;
      }
      // Touch: track drag for vertical scroll; resolve click on pointerUp if not moved
      const hitUser = hitUserRectsRef.current.find((r) => my >= r.y && my <= r.y + r.h);
      pendingClickRef.current = hitUser ? `user:${hitUser.id}` : null;
      dragRef.current = {
        active: true, startX: e.clientX, lastX: e.clientX,
        startY: e.clientY, lastY: e.clientY,
        scrollXStart: scrollX, moved: false, isTouch: true,
        area: "user", axis: null,
      };
      return;
    }

    // Task panel or header area
    pendingClickRef.current = null;
    for (const r of hitTaskRectsRef.current) {
      if (my >= r.y && my <= r.y + r.h) {
        pendingClickRef.current = r.id;
        break;
      }
    }

    const area = my < HEADER_H + UTIL_ROW_H ? "header" : "task";
    dragRef.current = {
      active: true, startX: e.clientX, lastX: e.clientX,
      startY: e.clientY, lastY: e.clientY,
      scrollXStart: scrollX, moved: false,
      isTouch: e.pointerType !== "mouse",
      area, axis: null,
    };
  };

  const onPointerMove = (e: React.PointerEvent<HTMLCanvasElement>) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    mousePosRef.current = { x: mx, y: my };
    const epw = panelCollapsed ? 0 : USER_PANEL_W;

    if (e.pointerType === "mouse") {
      if (mx >= epw) {
        let hit: string | null = null;
        for (const r of hitViolationRectsRef.current) {
          if (mx >= r.x && mx <= r.x + r.r && my >= r.y && my <= r.y + ROW_H) {
            hit = `!:${r.id}`; break;
          }
        }
        if (!hit) {
          for (const r of hitTaskRectsRef.current) {
            if (my >= r.y && my <= r.y + r.h) { hit = r.id; break; }
          }
        }
        setHoveredTaskId(hit);
      } else {
        setHoveredTaskId(null);
      }
    }

    if (dragRef.current.active) {
      const dx = e.clientX - dragRef.current.lastX;
      const dy = e.clientY - dragRef.current.lastY;
      const totalDX = Math.abs(e.clientX - dragRef.current.startX);
      const totalDY = Math.abs(e.clientY - dragRef.current.startY);
      const totalMove = Math.max(totalDX, totalDY);
      const moveThreshold = dragRef.current.isTouch ? 8 : 5;

      if (totalMove > moveThreshold) {
        dragRef.current.moved = true;
        pendingClickRef.current = null;
      }

      // Lock touch drag axis after 12px movement
      if (dragRef.current.isTouch && dragRef.current.axis === null && totalMove > 12) {
        dragRef.current.axis = totalDX >= totalDY ? "h" : "v";
      }

      const { area, axis, isTouch } = dragRef.current;

      if (area === "user") {
        // Vertical scroll in user panel (touch only)
        dragRef.current.lastY = e.clientY;
        const maxU = Math.max(0, users.length * ROW_H - size.h);
        setUserScrollY((sy) => Math.max(0, Math.min(maxU, sy - dy)));
      } else if (!isTouch || area === "header") {
        // Mouse always scrolls horizontally; header area is horizontal-only
        dragRef.current.lastX = e.clientX;
        setScrollX((sx) => Math.max(0, sx - dx));
      } else if (area === "task") {
        if (axis === "v") {
          dragRef.current.lastY = e.clientY;
          const maxT = Math.max(0, userTasks.length * ROW_H - (size.h - HEADER_H - UTIL_ROW_H));
          setTaskScrollY((sy) => Math.max(0, Math.min(maxT, sy - dy)));
        } else if (axis === "h") {
          dragRef.current.lastX = e.clientX;
          setScrollX((sx) => Math.max(0, sx - dx));
        } else {
          // Axis not yet locked — keep positions current so scroll begins from here when axis locks
          dragRef.current.lastX = e.clientX;
          dragRef.current.lastY = e.clientY;
        }
      }
    }
  };

  const onPointerUp = () => {
    const pending = pendingClickRef.current;
    if (!dragRef.current.moved && pending) {
      if (pending.startsWith("user:")) {
        const userId = pending.slice(5) as UserId;
        setSelectedUserId((prev) => (prev === userId ? null : userId));
        // Auto-collapse panel after selection on narrow screens
        if (autoCollapsed) setUserPanelOverride(null);
      } else if (!pending.startsWith("!:")) {
        setEditTaskId(pending as TaskId);
      }
    }
    dragRef.current.active = false;
    dragRef.current.moved = false;
    dragRef.current.axis = null;
    pendingClickRef.current = null;
  };

  const onPointerCancel = () => {
    dragRef.current.active = false;
    dragRef.current.moved = false;
    dragRef.current.axis = null;
    pendingClickRef.current = null;
    setHoveredTaskId(null);
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const epw = panelCollapsed ? 0 : USER_PANEL_W;
    if (mx < epw) {
      const maxU = Math.max(0, users.length * ROW_H - size.h);
      setUserScrollY((sy) => Math.max(0, Math.min(maxU, sy + e.deltaY)));
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
      <button
        className="allocation-panel-toggle"
        onClick={() => setUserPanelOverride(!panelCollapsed)}
        title={panelCollapsed ? "Show users panel" : "Hide users panel"}
      >
        {panelCollapsed ? "☰" : "✕"}
      </button>
      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block", cursor: dragRef.current.active ? "grabbing" : hoveredTaskId ? "pointer" : "default", touchAction: "none" }}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerCancel}
        onPointerLeave={(e) => { if (e.pointerType === "mouse") onPointerCancel(); }}
      />

      {editTaskId && plan?.tasks[editTaskId] && (
        <TaskFormModal
          task={plan.tasks[editTaskId]}
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setEditTaskId(null)}
        />
      )}
      {showUsers && plan && null /* users managed via Resources page */}
    </div>
  );
}
