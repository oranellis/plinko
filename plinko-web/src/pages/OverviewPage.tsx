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
import "./OverviewPage.css";

const ROW_H = 36;
const HEADER_H = 46; // month (18) + day (28)
const DAY_W_DEFAULT = 28;
const MIN_DAY_W = 8;
const MAX_DAY_W = 80;

export function OverviewPage() {
  const { plan, sendRequest } = usePlanContext();
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

  // Hit-test refs (populated during render)
  const hitRectsRef = useRef<{ id: string; x: number; y: number; w: number; h: number }[]>([]);

  // Canvas render
  const render = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas || !plan) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const { w, h } = size;
    canvas.width = w;
    canvas.height = h;
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

    // Month header (top 18px)
    ctx.font = "11px sans-serif";
    ctx.fillStyle = "#888";
    let curMonthLabel = "";
    let monthStartX = 0;
    for (let d = firstVisibleDay; d <= lastVisibleDay; d++) {
      const date = parseDate(addDays(startDate, d));
      const label = date.toLocaleString("default", { month: "short", year: "numeric" });
      const x = d * dayW - scrollX;
      if (label !== curMonthLabel) {
        if (curMonthLabel) {
          ctx.fillText(curMonthLabel, monthStartX + 4, 13);
          ctx.strokeStyle = "#3a3a3c";
          ctx.beginPath(); ctx.moveTo(x, 0); ctx.lineTo(x, 18); ctx.stroke();
        }
        curMonthLabel = label;
        monthStartX = x;
      }
    }
    if (curMonthLabel) ctx.fillText(curMonthLabel, monthStartX + 4, 13);

    // Day header (bottom 28px of header)
    ctx.font = "10px sans-serif";
    for (let d = firstVisibleDay; d <= lastVisibleDay; d++) {
      const date = parseDate(addDays(startDate, d));
      const x = d * dayW - scrollX;
      const dayNum = date.getDate();
      const isWeekend = date.getDay() === 0 || date.getDay() === 6;

      ctx.fillStyle = isWeekend ? "#1e1e1e" : "#252526";
      ctx.fillRect(x, 18, dayW, 28);

      ctx.fillStyle = d === todayOffset ? "#4a90d9" : (isWeekend ? "#555" : "#aaa");
      ctx.textAlign = "center";
      ctx.fillText(String(dayNum), x + dayW / 2, 36);
      ctx.textAlign = "left";

      ctx.strokeStyle = "#2a2a2c";
      ctx.beginPath(); ctx.moveTo(x, 18); ctx.lineTo(x, h); ctx.stroke();
    }

    // Header bottom border
    ctx.strokeStyle = "#3a3a3c";
    ctx.beginPath(); ctx.moveTo(0, HEADER_H); ctx.lineTo(w, HEADER_H); ctx.stroke();

    // === GANTT ROWS ===
    const clipTop = HEADER_H;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, clipTop, w, h - clipTop);
    ctx.clip();

    for (const item of items) {
      const rowY = HEADER_H + item.row * ROW_H - scrollY;
      if (rowY + ROW_H < HEADER_H || rowY > h) continue;

      const startOff = daysBetween(startDate, item.start);
      const endOff = daysBetween(startDate, item.end) + 1;
      const x = startOff * dayW - scrollX;
      const barW = Math.max((endOff - startOff) * dayW, 4);
      const y = rowY + 4;
      const barH = ROW_H - 8;

      const color = STATUS_COLORS[item.status];
      const isHovered = hoverId === item.id;
      const isFlashing = flashId === item.id;

      if (item.type === "task") {
        // Rounded rect bar
        ctx.fillStyle = isHovered || isFlashing ? lighten(color) : color;
        roundRect(ctx, x, y, barW, barH, 4);
        ctx.fill();

        // Label
        const label = displayName(item.name, item.contextLabel);
        ctx.fillStyle = "#fff";
        ctx.font = "11px sans-serif";
        ctx.save();
        ctx.beginPath();
        ctx.rect(x + 4, y, barW - 8, barH);
        ctx.clip();
        ctx.fillText(label, x + 6, y + barH / 2 + 4);
        ctx.restore();
      } else {
        // Milestone diamond
        const cx = x + dayW / 2;
        const cy = rowY + ROW_H / 2;
        const r = 7;
        ctx.fillStyle = isHovered || isFlashing ? lighten("#e0c040") : "#e0c040";
        ctx.beginPath();
        ctx.moveTo(cx, cy - r);
        ctx.lineTo(cx + r, cy);
        ctx.lineTo(cx, cy + r);
        ctx.lineTo(cx - r, cy);
        ctx.closePath();
        ctx.fill();
      }

      // Register hit rect
      hitRectsRef.current.push({ id: item.id, x, y, w: barW, h: barH });
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

      setScrollX((sx) => Math.max(0, sx - dx));
      setScrollY((sy) => Math.max(0, sy - dy));
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
      setScrollX((sx) => Math.max(0, sx - vx * 0.016));
      setScrollY((sy) => Math.max(0, sy - vy * 0.016));
      momRef.current = requestAnimationFrame(step);
    };
    momRef.current = requestAnimationFrame(step);
  };

  const onWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    if (e.shiftKey) {
      // Zoom
      const factor = e.deltaY > 0 ? 0.9 : 1.1;
      setDayW((w) => Math.max(MIN_DAY_W, Math.min(MAX_DAY_W, w * factor)));
    } else {
      setScrollX((sx) => Math.max(0, sx + e.deltaX));
      setScrollY((sy) => Math.max(0, sy + e.deltaY));
    }
  };

  const handleSearchSelect = (id: string) => {
    setShowSearch(false);
    if (!plan) return;
    const item = items.find((it) => it.id === id);
    if (!item) return;
    const offset = daysBetween(plan.start_date, item.start);
    setScrollX(Math.max(0, offset * dayW - size.w / 2));
    setScrollY(item.row * ROW_H - size.h / 2);
    setFlashId(id);
    let count = 0;
    const flash = () => {
      setFlashId(() => (count++ % 2 === 0 ? id : null));
      if (count < 6) setTimeout(flash, 250);
    };
    setTimeout(flash, 100);
  };

  return (
    <div className="overview-page" ref={containerRef}>
      {/* Toolbar */}
      <div className="overview-toolbar">
        <button className="overview-tool-btn" title="Today"
          onClick={() => {
            if (!plan) return;
            const today = formatDate(new Date());
            const offset = daysBetween(plan.start_date, today);
            setScrollX(Math.max(0, offset * dayW - size.w / 2));
          }}
        >📅 Today</button>
        <button className="overview-tool-btn" onClick={() => setEditTaskId("new")}>＋ Task</button>
        <button className="overview-tool-btn" onClick={() => setEditMsId("new")}>◇ Milestone</button>
        <button className="overview-tool-btn" onClick={() => setShowSearch(true)}>⌕ Search</button>
        <span className="overview-toolbar-spacer" />
        <button className="overview-tool-btn" onClick={() => setShowUsers(true)}>👤 Users</button>
        <button className="overview-tool-btn" onClick={() => setShowSettings(true)}>⚙ Settings</button>
      </div>

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
        onWheel={onWheel}
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

function lighten(hex: string): string {
  const c = parseInt(hex.slice(1), 16);
  const r = Math.min(255, ((c >> 16) & 0xff) + 40);
  const g = Math.min(255, ((c >> 8) & 0xff) + 40);
  const b = Math.min(255, (c & 0xff) + 40);
  return `rgb(${r},${g},${b})`;
}
