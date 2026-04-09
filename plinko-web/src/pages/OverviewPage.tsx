import { useCallback, useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import type { MilestoneId, TaskId } from "../protocol";
import {
  STATUS_COLORS,
  addDays,
  daysBetween,
  displayName,
  formatDate,
  packGanttRows,
  parseDate,
} from "../utils/planUtils";
import { TaskFormModal } from "../components/modals/TaskFormModal";
import { MilestoneFormModal } from "../components/modals/MilestoneFormModal";
import { UsersModal } from "../components/modals/UsersModal";
import { PlanSettingsModal } from "../components/modals/PlanSettingsModal";
import { SearchModal } from "../components/modals/SearchModal";
import {
  IconAddMilestone,
  IconAddTask,
  IconSearch,
  IconSettings,
  IconToday,
  IconUsers,
} from "../components/icons";
import "./OverviewPage.css";

const ROW_H = 36;
const HEADER_H = 56; // month (22) + day (34)
const DAY_W_DEFAULT = 34;
const MIN_DAY_W = 8;
const MAX_DAY_W = 80;

export function OverviewPage() {
  const { plan, sendRequest, setToolbarActions, setToolbarRightActions } = usePlanContext();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // View state
  const [scrollX, setScrollX] = useState(0); // pixels from plan start
  const [scrollY, setScrollY] = useState(0);
  const [dayW, setDayW] = useState(DAY_W_DEFAULT);
  const [size, setSize] = useState({ w: 800, h: 600 });

  // Modals
  const [editTaskId, setEditTaskId] = useState<TaskId | null | "new">(null);
  const [editMsId, setEditMsId] = useState<MilestoneId | null | "new">(null);
  const [showUsers, setShowUsers] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [showSearch, setShowSearch] = useState(false);

  // Hover / flash
  const [hoverId, setHoverId] = useState<string | null>(null);
  const [flashId, setFlashId] = useState<string | null>(null);

  // Drag momentum
  const dragRef = useRef({ active: false, startX: 0, startY: 0, velX: 0, velY: 0, lastX: 0, lastY: 0, lastT: 0 });
  const momRef = useRef<number | null>(null);

  // Register toolbar action buttons; need refs to avoid stale closures
  const setEditTaskIdRef = useRef(setEditTaskId);
  const setEditMsIdRef = useRef(setEditMsId);
  const setShowUsersRef = useRef(setShowUsers);
  const setShowSettingsRef = useRef(setShowSettings);
  const setShowSearchRef = useRef(setShowSearch);
  const planRef = useRef(plan);
  const scrollXRef = useRef(scrollX);
  const sizeRef = useRef(size);
  const dayWRef = useRef(dayW);
  setEditTaskIdRef.current = setEditTaskId;
  setEditMsIdRef.current = setEditMsId;
  setShowUsersRef.current = setShowUsers;
  setShowSettingsRef.current = setShowSettings;
  setShowSearchRef.current = setShowSearch;
  planRef.current = plan;
  scrollXRef.current = scrollX;
  sizeRef.current = size;
  dayWRef.current = dayW;

  useEffect(() => {
    setToolbarActions(
      <>
        <button className="toolbar-btn" title="Jump to today" onClick={() => {
          const p = planRef.current; const sw = sizeRef.current.w; const dw = dayWRef.current;
          if (!p) return;
          const today = formatDate(new Date());
          const offset = daysBetween(p.start_date, today);
          setScrollX(Math.max(-sw / 2, offset * dw - sw / 2));
        }}><IconToday size={24} /></button>
        <button className="toolbar-btn" title="Add task" onClick={() => setEditTaskIdRef.current("new")}><IconAddTask size={24} /></button>
        <button className="toolbar-btn" title="Add milestone" onClick={() => setEditMsIdRef.current("new")}><IconAddMilestone size={24} /></button>
        <button className="toolbar-btn" title="Search" onClick={() => setShowSearchRef.current(true)}><IconSearch size={24} /></button>
      </>
    );
    setToolbarRightActions(
      <>
        <button className="toolbar-btn" title="Users" onClick={() => setShowUsersRef.current(true)}><IconUsers size={24} /></button>
        <button className="toolbar-btn" title="Settings" onClick={() => setShowSettingsRef.current(true)}><IconSettings size={24} /></button>
      </>
    );
    return () => { setToolbarActions(null); setToolbarRightActions(null); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  // Resize observer
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
    setScrollX(Math.max(0, offset * DAY_W_DEFAULT - size.w / 2));
    setScrollY(0);
  }, [plan?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const items = plan ? packGanttRows(plan) : [];

  // Scroll limits
  const maxRows = items.length > 0 ? Math.max(...items.map((i) => i.row)) + 1 : 1;
  const maxScrollY = Math.max(0, maxRows * ROW_H - (size.h - HEADER_H));
  const maxScrollX = items.length > 0
    ? Math.max(...items.filter((i) => i.type !== "separator").map((i) => {
        const off = daysBetween(plan!.start_date, (i as { end: string }).end) + 1;
        return off * dayW;
      })) + size.w / 2
    : size.w;

  // Hit-test refs (populated during render)
  const hitRectsRef = useRef<{ id: string; x: number; y: number; w: number; h: number }[]>([]);

  // Canvas render
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
    hitRectsRef.current = [];

    ctx.clearRect(0, 0, w, h);
    ctx.fillStyle = "#1e1e1e";
    ctx.fillRect(0, 0, w, h);

    const startDate = plan.start_date;
    const today = formatDate(new Date());
    const todayOffset = daysBetween(startDate, today);

    // Today column
    const todayX = todayOffset * dayW - scrollX;
    if (todayX >= 0 && todayX < w) {
      ctx.fillStyle = "rgba(64,91,200,0.18)";
      ctx.fillRect(todayX, 0, dayW, h);
    }

    // === HEADERS ===
    ctx.fillStyle = "#252526";
    ctx.fillRect(0, 0, w, HEADER_H);

    // Determine visible date range
    const firstVisibleDay = Math.floor(scrollX / dayW);
    const lastVisibleDay = Math.ceil((scrollX + w) / dayW);

    // Month header (top 22px)
    ctx.font = "13px sans-serif";
    ctx.fillStyle = "#888";
    let curMonthLabel = "";
    let monthStartX = 0;
    for (let d = firstVisibleDay; d <= lastVisibleDay; d++) {
      const date = parseDate(addDays(startDate, d));
      const label = date.toLocaleString("default", { month: "short", year: "numeric" });
      const x = d * dayW - scrollX;
      if (label !== curMonthLabel) {
        if (curMonthLabel) {
          ctx.fillText(curMonthLabel, monthStartX + 4, 15);
          ctx.strokeStyle = "#3a3a3c";
          ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, 22); ctx.stroke();
        }
        curMonthLabel = label;
        monthStartX = x;
      }
    }
    if (curMonthLabel) ctx.fillText(curMonthLabel, monthStartX + 4, 15);

    // Day header (bottom 34px of header)
    ctx.font = "12px sans-serif";
    for (let d = firstVisibleDay; d <= lastVisibleDay; d++) {
      const date = parseDate(addDays(startDate, d));
      const x = d * dayW - scrollX;
      const dayNum = date.getDate();
      const isWeekend = date.getDay() === 0 || date.getDay() === 6;

      ctx.fillStyle = isWeekend ? "#1e1e1e" : "#252526";
      ctx.fillRect(x, 22, dayW, 34);

      ctx.fillStyle = d === todayOffset ? "#4a90d9" : (isWeekend ? "#555" : "#aaa");
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(String(dayNum), x + dayW / 2, 39);
      ctx.textAlign = "left";
      ctx.textBaseline = "alphabetic";

      ctx.strokeStyle = "#2a2a2c";
      ctx.beginPath(); ctx.moveTo(x, 22); ctx.lineTo(x, h); ctx.stroke();
    }

    // Header bottom border
    ctx.strokeStyle = "#3a3a3c";
    ctx.beginPath(); ctx.moveTo(0, HEADER_H); ctx.lineTo(w, HEADER_H); ctx.stroke();

    // Pre-compute dependency sets for hover highlighting
    const hoveredDeps = new Set<string>();   // items the hovered item depends on
    const hoveredDependents = new Set<string>(); // items that depend on the hovered item
    if (hoverId && plan) {
      const getDeps = (id: string): string[] => {
        const task = plan.tasks[id as TaskId];
        if (task) return task.dependencies.map((d) =>
          typeof d.id === "object" && "Task" in d.id ? d.id.Task :
          typeof d.id === "object" && "Milestone" in d.id ? d.id.Milestone : "");
        const ms = plan.milestones[id as MilestoneId];
        if (ms) return ms.dependencies.map((d) =>
          typeof d.id === "object" && "Task" in d.id ? d.id.Task :
          typeof d.id === "object" && "Milestone" in d.id ? d.id.Milestone : "");
        return [];
      };
      for (const depId of getDeps(hoverId)) {
        if (depId) hoveredDeps.add(depId);
      }
      // Compute forward dependents
      for (const [tid, task] of Object.entries(plan.tasks)) {
        if (task.dependencies.some((d) =>
          (typeof d.id === "object" && "Task" in d.id && d.id.Task === hoverId) ||
          (typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === hoverId)
        )) hoveredDependents.add(tid);
      }
      for (const [mid, ms] of Object.entries(plan.milestones)) {
        if (ms.dependencies.some((d) =>
          (typeof d.id === "object" && "Task" in d.id && d.id.Task === hoverId) ||
          (typeof d.id === "object" && "Milestone" in d.id && d.id.Milestone === hoverId)
        )) hoveredDependents.add(mid);
      }
    }

    // === GANTT ROWS ===
    const clipTop = HEADER_H;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, clipTop, w, h - clipTop);
    ctx.clip();

    // Map from item id → day-center anchors for dependency arrow drawing
    // xIn = center of start-day column, xOut = center of end-day column, y = row center
    const itemCenters = new Map<string, { xIn: number; xOut: number; y: number }>();

    const BAR_PAD_Y = Math.round(ROW_H * 0.12);
    const BAR_PAD_X = dayW * 0.12;
    const barH = ROW_H - BAR_PAD_Y * 2;
    const targetId = plan.scheduler_target
      ? (typeof plan.scheduler_target === "object" && "Task" in plan.scheduler_target ? plan.scheduler_target.Task :
         typeof plan.scheduler_target === "object" && "Milestone" in plan.scheduler_target ? plan.scheduler_target.Milestone : null)
      : null;

    // Pre-compute next-item start x per row (for milestone text clipping)
    const nextItemX = new Map<string, number>();
    const rowBuckets = new Map<number, Array<{ id: string; startX: number }>>();
    for (const it of items) {
      if (it.type === "separator") continue;
      const sx = daysBetween(startDate, it.start) * dayW - scrollX;
      if (!rowBuckets.has(it.row)) rowBuckets.set(it.row, []);
      rowBuckets.get(it.row)!.push({ id: it.id, startX: sx });
    }
    for (const arr of rowBuckets.values()) {
      arr.sort((a, b) => a.startX - b.startX);
      for (let i = 0; i < arr.length; i++) {
        nextItemX.set(arr[i].id, i + 1 < arr.length ? arr[i + 1].startX : w + 9999);
      }
    }

    // Pass 1: pre-compute day-center anchors for all non-separator, non-dropped items
    const droppedIds = new Set(items.filter((it) => it.type !== "separator" && it.status === "Dropped").map((it) => it.id));
    for (const item of items) {
      if (item.type === "separator" || item.status === "Dropped") continue;
      const rowY = HEADER_H + item.row * ROW_H - scrollY;
      const startOff = daysBetween(startDate, item.start);
      const endOff = daysBetween(startDate, item.end) + 1;
      const xIn = startOff * dayW + dayW / 2 - scrollX;
      const xOut = (endOff - 1) * dayW + dayW / 2 - scrollX;
      itemCenters.set(item.id, { xIn, xOut, y: rowY + ROW_H / 2 });
    }

    // Pass 2: draw dependency lines (behind items), skipping dropped tasks
    if (hoverId && itemCenters.has(hoverId) && !droppedIds.has(hoverId)) {
      const hc = itemCenters.get(hoverId)!;
      ctx.lineWidth = 1.5;
      ctx.setLineDash([4, 3]);

      for (const depId of hoveredDeps) {
        if (droppedIds.has(depId)) continue;
        const dc = itemCenters.get(depId);
        if (!dc) continue;
        ctx.strokeStyle = "rgba(252,30,241,0.66)";
        ctx.beginPath();
        ctx.moveTo(dc.xOut, dc.y);
        ctx.lineTo(hc.xIn, hc.y);
        ctx.stroke();
      }

      for (const depId of hoveredDependents) {
        if (droppedIds.has(depId)) continue;
        const dc = itemCenters.get(depId);
        if (!dc) continue;
        ctx.strokeStyle = "rgba(7,252,215,0.66)";
        ctx.beginPath();
        ctx.moveTo(hc.xOut, hc.y);
        ctx.lineTo(dc.xIn, dc.y);
        ctx.stroke();
      }

      ctx.setLineDash([]);
      ctx.lineWidth = 1;
    }

    // Pass 3: draw items on top of dep lines
    for (const item of items) {
      const rowY = HEADER_H + item.row * ROW_H - scrollY;
      if (rowY + ROW_H < HEADER_H || rowY > h) continue;

      // Separator row: draw a subtle horizontal rule across the full width
      if (item.type === "separator") {
        const lineY = rowY + ROW_H / 2;
        ctx.strokeStyle = "rgba(138,138,138,0.31)";
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(0, lineY);
        ctx.lineTo(w, lineY);
        ctx.stroke();
        continue;
      }

      const startOff = daysBetween(startDate, item.start);
      const endOff = daysBetween(startDate, item.end) + 1;
      const x = startOff * dayW - scrollX + BAR_PAD_X;
      const barW = Math.max((endOff - startOff) * dayW - 2 * BAR_PAD_X, 4);
      const y = rowY + BAR_PAD_Y;

      const color = STATUS_COLORS[item.status];
      const isHovered = hoverId === item.id;
      const isFlashing = flashId === item.id;
      const isDepOf = hoveredDeps.has(item.id);
      const isDependent = hoveredDependents.has(item.id);
      const isTarget = targetId === item.id;

      if (item.type === "task") {
        ctx.fillStyle = color;
        roundRect(ctx, x, y, barW, barH, 6);
        ctx.fill();

        // Border: coloured for dep relationships, gold for target, red flash
        const borderColor = isFlashing ? "#e53935"
          : isTarget ? "#ffd600"
          : isHovered ? "#1e88e5"
          : isDepOf ? "#fc1ef1"
          : isDependent ? "#07fcd7"
          : null;
        if (borderColor) {
          ctx.strokeStyle = borderColor;
          ctx.lineWidth = isFlashing ? 4 : isTarget ? 3 : isHovered ? 3.5 : 3;
          roundRect(ctx, x, y, barW, barH, 6);
          ctx.stroke();
          ctx.lineWidth = 1;
        }

        // Label
        const label = displayName(item.name, item.contextLabel);
        const isInProgress = item.status === "InProgress";
        ctx.fillStyle = isInProgress ? "rgba(0,0,0,0.85)" : "rgba(255,255,255,0.9)";
        ctx.font = "16px sans-serif";
        ctx.textBaseline = "middle";
        ctx.save();
        ctx.beginPath();
        ctx.rect(x + 4, y, barW - 8, barH);
        ctx.clip();
        ctx.fillText(label, x + 6, y + barH / 2);
        ctx.restore();
        ctx.textBaseline = "alphabetic";

        hitRectsRef.current.push({ id: item.id, x, y, w: barW, h: barH });
      } else {
        // Milestone diamond
        const cx = x + dayW / 2;
        const cy = rowY + ROW_H / 2;
        const r = 10;
        ctx.fillStyle = "#e0c040";
        ctx.beginPath();
        ctx.moveTo(cx, cy - r);
        ctx.lineTo(cx + r, cy);
        ctx.lineTo(cx, cy + r);
        ctx.lineTo(cx - r, cy);
        ctx.closePath();
        ctx.fill();

        // Border
        const borderColor = isFlashing ? "#e53935"
          : isTarget ? "#ffd600"
          : isHovered ? "#1e88e5"
          : isDepOf ? "#fc1ef1"
          : isDependent ? "#07fcd7"
          : null;
        if (borderColor) {
          ctx.strokeStyle = borderColor;
          ctx.lineWidth = isFlashing ? 4 : isTarget ? 3 : isHovered ? 3.5 : 3;
          ctx.beginPath();
          ctx.moveTo(cx, cy - r);
          ctx.lineTo(cx + r, cy);
          ctx.lineTo(cx, cy + r);
          ctx.lineTo(cx - r, cy);
          ctx.closePath();
          ctx.stroke();
          ctx.lineWidth = 1;
        }

        // Milestone name to the right — clipped at the next item in this row
        const nameX = cx + r + 6;
        const nameClipEnd = Math.min((nextItemX.get(item.id) ?? w) - 4, w);
        const nameVisible = nameX < nameClipEnd;
        if (nameVisible) {
          ctx.fillStyle = isHovered ? "#f5d040" : "#bbb";
          ctx.font = "14px sans-serif";
          ctx.textBaseline = "middle";
          ctx.save();
          ctx.beginPath();
          ctx.rect(nameX, rowY, nameClipEnd - nameX, ROW_H);
          ctx.clip();
          ctx.fillText(displayName(item.name, item.contextLabel), nameX, rowY + ROW_H / 2);
          ctx.restore();
          ctx.textBaseline = "alphabetic";
        }

        hitRectsRef.current.push({ id: item.id, x: cx - r, y: cy - r, w: r * 2, h: r * 2 });
      }
    }

    // === HOVER INFO PANEL (bottom-left) ===
    if (hoverId) {
      const panelPad = 10;
      const margin = 8;
      const lineGap = 4;
      const titleSize = 20;
      const bodySize = 15;

      // Build lines
      const lines: string[] = [];
      const taskId = hoverId as import("../protocol").TaskId;
      const msId = hoverId as import("../protocol").MilestoneId;
      const task = plan.tasks[taskId];
      const ms = plan.milestones[msId];

      if (task) {
        const tname = task.context_label ? `${task.name} | ${task.context_label}` : task.name;
        lines.push(tname);
        const taskState = plan.node_allocations.tasks[taskId];
        const status = taskState?.status ?? "Unknown";
        lines.push(`Status: ${status}`);
        if (task.actual_start) {
          lines.push(`Started: ${task.actual_start}`);
        } else if (taskState) {
          const alloc = taskState.allocation;
          const sched = "Fixed" in alloc ? alloc.Fixed.start_date : alloc.Dynamic.scheduled_start_date;
          lines.push(`Scheduled: ${sched}`);
        }
        // End date
        if (taskState) {
          const alloc = taskState.allocation;
          const endDate = "Fixed" in alloc ? alloc.Fixed.end_date : alloc.Dynamic.scheduled_end_date;
          lines.push(`Ends: ${endDate}`);
        }
        // Workers
        const workerNames = task.workers.flatMap((slot) => {
          if ("Specific" in slot) {
            const user = plan.users_data[slot.Specific.user_id];
            return user ? [user.user.name] : [];
          } else {
            const tagNames = slot.Placeholder.required_tags
              .map((tid) => plan.tags.find((t) => t.id === tid)?.name)
              .filter(Boolean).join(", ");
            return tagNames ? [`needs: ${tagNames}`] : ["(unassigned)"];
          }
        });
        if (workerNames.length > 0) lines.push(`Workers: ${workerNames.join(", ")}`);
      } else if (ms) {
        const mname = ms.context_label ? `${ms.name} | ${ms.context_label}` : ms.name;
        lines.push(mname);
        lines.push("Milestone");
        const msState = plan.node_allocations.milestones[msId];
        if (msState) lines.push(`Scheduled: ${msState.date}`);
      }

      if (lines.length > 0) {
        ctx.save();
        ctx.resetTransform();
        ctx.scale(dpr, dpr);

        // Measure
        ctx.font = `${titleSize}px sans-serif`;
        const titleW = ctx.measureText(lines[0]).width;
        ctx.font = `${bodySize}px sans-serif`;
        const bodyMaxW = lines.slice(1).reduce((mx, l) => Math.max(mx, ctx.measureText(l).width), 0);
        const panelW = Math.max(titleW, bodyMaxW) + panelPad * 2;
        const panelH = panelPad * 2
          + (titleSize * 1.25)
          + (lines.length > 1 ? lineGap + (lines.length - 1) * (bodySize * 1.4) + (lines.length - 2) * lineGap : 0);

        const px = margin;
        const py = h - margin - panelH;

        // Shadow
        ctx.fillStyle = "rgba(0,0,0,0.19)";
        ctx.beginPath();
        roundRect(ctx, px + 2, py + 3, panelW, panelH, 6);
        ctx.fill();

        // Background
        ctx.fillStyle = "rgba(30,30,30,0.86)";
        roundRect(ctx, px, py, panelW, panelH, 6);
        ctx.fill();

        // Border
        ctx.strokeStyle = "#4a4a4a";
        ctx.lineWidth = 1;
        roundRect(ctx, px, py, panelW, panelH, 6);
        ctx.stroke();

        // Title line
        ctx.font = `${titleSize}px sans-serif`;
        ctx.fillStyle = "#d4d4d4";
        ctx.textBaseline = "top";
        ctx.fillText(lines[0], px + panelPad, py + panelPad);

        // Body lines
        ctx.font = `${bodySize}px sans-serif`;
        ctx.fillStyle = "#8a8a8a";
        const bodyStartY = py + panelPad + titleSize * 1.25 + lineGap;
        for (let i = 0; i < lines.length - 1; i++) {
          ctx.fillText(lines[i + 1], px + panelPad, bodyStartY + i * (bodySize * 1.4 + lineGap));
        }

        ctx.restore();
      }
    }

    ctx.restore();
  }, [plan, items, size, scrollX, scrollY, dayW, hoverId, flashId]);

  useEffect(() => { render(); }, [render]);

  // Mouse events
  const hitTest = (mx: number, my: number): string | null => {
    for (const r of hitRectsRef.current) {
      if (mx >= r.x && mx <= r.x + r.w && my >= r.y && my <= r.y + r.h) return r.id;
    }
    return null;
  };

  const onMouseMove = (e: React.MouseEvent) => {
    const rect = canvasRef.current!.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const hit = hitTest(mx, my);
    setHoverId(hit);

    if (dragRef.current.active) {
      const now = Date.now();
      const dt = now - dragRef.current.lastT;
      const dx = e.clientX - dragRef.current.lastX;
      const dy = e.clientY - dragRef.current.lastY;
      dragRef.current.velX = dt > 0 ? dx / dt : 0;
      dragRef.current.velY = dt > 0 ? dy / dt : 0;
      dragRef.current.lastX = e.clientX;
      dragRef.current.lastY = e.clientY;
      dragRef.current.lastT = now;

      setScrollX((sx) => Math.max(-size.w / 2, Math.min(maxScrollX, sx - dx)));
      setScrollY((sy) => Math.max(0, Math.min(maxScrollY, sy - dy)));
    }
  };

  const onMouseDown = (e: React.MouseEvent) => {
    if (momRef.current) cancelAnimationFrame(momRef.current);
    dragRef.current = { active: true, startX: e.clientX, startY: e.clientY, velX: 0, velY: 0, lastX: e.clientX, lastY: e.clientY, lastT: Date.now() };
  };

  const onMouseUp = (e: React.MouseEvent) => {
    const d = dragRef.current;
    d.active = false;

    const dx = Math.abs(e.clientX - d.startX);
    const dy = Math.abs(e.clientY - d.startY);
    if (dx < 4 && dy < 4) {
      // Click — open modal
      const rect = canvasRef.current!.getBoundingClientRect();
      const id = hitTest(e.clientX - rect.left, e.clientY - rect.top);
      if (id && plan) {
        if (plan.tasks[id as TaskId]) setEditTaskId(id as TaskId);
        else if (plan.milestones[id as MilestoneId]) setEditMsId(id as MilestoneId);
      }
      return;
    }

    // Momentum
    let vx = d.velX * 1000;
    let vy = d.velY * 1000;
    const friction = 0.85;
    const step = () => {
      vx *= friction;
      vy *= friction;
      if (Math.abs(vx) < 0.5 && Math.abs(vy) < 0.5) return;
      setScrollX((sx) => Math.max(-size.w / 2, Math.min(maxScrollX, sx - vx * 0.016)));
      setScrollY((sy) => Math.max(0, Math.min(maxScrollY, sy - vy * 0.016)));
      momRef.current = requestAnimationFrame(step);
    };
    momRef.current = requestAnimationFrame(step);
  };

  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    if (e.shiftKey) {
      const factor = e.deltaY > 0 ? 0.9 : 1.1;
      const newDayW = Math.max(MIN_DAY_W, Math.min(MAX_DAY_W, dayW * factor));
      const rect = canvasRef.current!.getBoundingClientRect();
      const cursorX = e.clientX - rect.left;
      // Keep the day under the cursor fixed
      const dayAtCursor = (scrollX + cursorX) / dayW;
      const newScrollX = dayAtCursor * newDayW - cursorX;
      setDayW(newDayW);
      setScrollX(Math.max(-size.w / 2, Math.min(maxScrollX, newScrollX)));
    } else {
      setScrollX((sx) => Math.max(-size.w / 2, Math.min(maxScrollX, sx + e.deltaX)));
      setScrollY((sy) => Math.max(0, Math.min(maxScrollY, sy + e.deltaY)));
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

  const handleSearchSelect = (id: string) => {
    setShowSearch(false);
    if (!plan) return;
    const item = items.find((it) => it.id === id);
    if (!item || item.type === "separator") return;
    const offset = daysBetween(plan.start_date, item.start);
    // Center task horizontally (task start at screen center) and vertically (row center at screen center)
    setScrollX(Math.max(-size.w / 2, Math.min(maxScrollX, offset * dayW - size.w / 2)));
    setScrollY(Math.max(0, Math.min(maxScrollY, item.row * ROW_H + ROW_H / 2 - (size.h - HEADER_H) / 2)));
    // 3 flashes over 3 seconds (on 500ms, off 500ms × 3)
    setFlashId(id);
    let count = 0;
    const flash = () => {
      count++;
      setFlashId(count % 2 === 0 ? id : null);
      if (count < 6) setTimeout(flash, 500);
    };
    setTimeout(flash, 500);
  };

  return (
    <div className="overview-page" ref={containerRef}>
      {/* Canvas */}
      <canvas
        ref={canvasRef}
        width={size.w}
        height={size.h}
        style={{ display: "block", cursor: hoverId ? "pointer" : "default" }}
        onMouseMove={onMouseMove}
        onMouseDown={onMouseDown}
        onMouseUp={onMouseUp}
        onMouseLeave={() => { dragRef.current.active = false; setHoverId(null); }}
      />

      {/* Modals */}
      {editTaskId === "new" && (
        <TaskFormModal
          task={null}
          plan={plan!}
          sendRequest={sendRequest}
          onClose={() => setEditTaskId(null)}
        />
      )}
      {editTaskId && editTaskId !== "new" && plan?.tasks[editTaskId] && (
        <TaskFormModal
          task={plan.tasks[editTaskId]}
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setEditTaskId(null)}
        />
      )}
      {editMsId === "new" && (
        <MilestoneFormModal
          milestone={null}
          plan={plan!}
          sendRequest={sendRequest}
          onClose={() => setEditMsId(null)}
        />
      )}
      {editMsId && editMsId !== "new" && plan?.milestones[editMsId] && (
        <MilestoneFormModal
          milestone={plan.milestones[editMsId]}
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setEditMsId(null)}
        />
      )}
      {showUsers && plan && (
        <UsersModal
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setShowUsers(false)}
        />
      )}
      {showSettings && plan && (
        <PlanSettingsModal
          plan={plan}
          sendRequest={sendRequest}
          onClose={() => setShowSettings(false)}
        />
      )}
      {showSearch && plan && (
        <SearchModal
          plan={plan}
          onSelect={handleSearchSelect}
          onClose={() => setShowSearch(false)}
        />
      )}
    </div>
  );
}

function roundRect(
  ctx: CanvasRenderingContext2D,
  x: number, y: number, w: number, h: number, r: number
) {
  r = Math.min(r, w / 2, h / 2);
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.lineTo(x + w - r, y);
  ctx.quadraticCurveTo(x + w, y, x + w, y + r);
  ctx.lineTo(x + w, y + h - r);
  ctx.quadraticCurveTo(x + w, y + h, x + w - r, y + h);
  ctx.lineTo(x + r, y + h);
  ctx.quadraticCurveTo(x, y + h, x, y + h - r);
  ctx.lineTo(x, y + r);
  ctx.quadraticCurveTo(x, y, x + r, y);
  ctx.closePath();
}
