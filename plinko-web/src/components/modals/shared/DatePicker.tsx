/**
 * Custom date picker with a calendar popup.
 * - Single chevrons (‹ ›) navigate months
 * - Double chevrons (« ») navigate years
 * - "Today" and "Clear" buttons at bottom
 * No browser native date input — fully custom popup.
 */
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

interface Props {
  value: string; // YYYY-MM-DD or ""
  onChange: (v: string) => void;
  disabled?: boolean;
  placeholder?: string;
}

const MONTHS = ["January","February","March","April","May","June",
                 "July","August","September","October","November","December"];
const DAYS = ["Su","Mo","Tu","We","Th","Fr","Sa"];

function today(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth()+1).padStart(2,"0")}-${String(d.getDate()).padStart(2,"0")}`;
}

function parseDate(s: string): { y: number; m: number; d: number } | null {
  if (!s) return null;
  const parts = s.split("-");
  if (parts.length !== 3) return null;
  return { y: parseInt(parts[0]), m: parseInt(parts[1]) - 1, d: parseInt(parts[2]) };
}

function formatDisplay(s: string): string {
  const p = parseDate(s);
  if (!p) return "";
  return `${String(p.d).padStart(2,"0")} ${MONTHS[p.m].slice(0,3)} ${p.y}`;
}

function daysInMonth(y: number, m: number): number {
  return new Date(y, m + 1, 0).getDate();
}

export function DatePicker({ value, onChange, disabled = false, placeholder = "Select date…" }: Props) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popupRef = useRef<HTMLDivElement>(null);

  const parsed = parseDate(value);
  const todayStr = today();
  const todayParsed = parseDate(todayStr)!;

  const [viewY, setViewY] = useState(parsed?.y ?? todayParsed.y);
  const [viewM, setViewM] = useState(parsed?.m ?? todayParsed.m);

  // Update view when value changes externally
  useEffect(() => {
    if (parsed) { setViewY(parsed.y); setViewM(parsed.m); }
  }, [value]); // eslint-disable-line react-hooks/exhaustive-deps

  const [pos, setPos] = useState({ top: 0, left: 0 });

  const openPopup = () => {
    if (disabled) return;
    if (triggerRef.current) {
      const rect = triggerRef.current.getBoundingClientRect();
      const popH = 280;
      const top = (window.innerHeight - rect.bottom >= popH) ? rect.bottom + 2 : rect.top - popH - 2;
      setPos({ top, left: rect.left });
    }
    setOpen(true);
  };

  // Close on outside click
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (
        popupRef.current && !popupRef.current.contains(e.target as Node) &&
        triggerRef.current && !triggerRef.current.contains(e.target as Node)
      ) setOpen(false);
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [open]);

  const prevMonth = () => {
    if (viewM === 0) { setViewM(11); setViewY(viewY - 1); }
    else setViewM(viewM - 1);
  };
  const nextMonth = () => {
    if (viewM === 11) { setViewM(0); setViewY(viewY + 1); }
    else setViewM(viewM + 1);
  };
  const prevYear = () => setViewY(viewY - 1);
  const nextYear = () => setViewY(viewY + 1);

  const selectDay = (d: number) => {
    const s = `${viewY}-${String(viewM+1).padStart(2,"0")}-${String(d).padStart(2,"0")}`;
    onChange(s);
    setOpen(false);
  };

  // Build calendar grid
  const firstDow = new Date(viewY, viewM, 1).getDay(); // 0=Sun
  const numDays = daysInMonth(viewY, viewM);
  const cells: (number | null)[] = [];
  for (let i = 0; i < firstDow; i++) cells.push(null);
  for (let d = 1; d <= numDays; d++) cells.push(d);

  const triggerStyle: React.CSSProperties = {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    background: disabled ? "#1a1a1c" : "#1e1e1e",
    border: `1px solid ${open ? "#4a90d9" : "#3a3a3c"}`,
    borderRadius: 4,
    color: disabled ? "#555" : value ? "#d4d4d4" : "#666",
    fontSize: 13,
    padding: "0 10px",
    height: 30,
    cursor: disabled ? "not-allowed" : "pointer",
    fontFamily: "inherit",
    width: "100%",
    boxSizing: "border-box",
  };

  const navBtnStyle: React.CSSProperties = {
    background: "none",
    border: "none",
    color: "#888",
    cursor: "pointer",
    fontSize: 18,
    padding: "4px 8px",
    fontFamily: "inherit",
    lineHeight: 1,
    borderRadius: 3,
  };

  return (
    <>
      <button ref={triggerRef} style={triggerStyle} onClick={openPopup} type="button">
        <span>{value ? formatDisplay(value) : placeholder}</span>
        <span style={{ color: "#555", fontSize: 11 }}>▾</span>
      </button>

      {open && createPortal(
        <div
          ref={popupRef}
          style={{
            position: "fixed",
            top: pos.top,
            left: pos.left,
            width: 240,
            zIndex: 9999,
            background: "#252526",
            border: "1px solid #4a90d9",
            borderRadius: 6,
            boxShadow: "0 4px 20px rgba(0,0,0,0.7)",
            padding: "10px",
            userSelect: "none",
          }}
        >
          {/* Header: year nav + month nav */}
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 8 }}>
            <div style={{ display: "flex", alignItems: "center", gap: 0 }}>
              <button style={navBtnStyle} onClick={prevYear} title="Previous year"
                onMouseEnter={(e) => (e.currentTarget.style.color = "#d4d4d4")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "#888")}>«</button>
              <button style={navBtnStyle} onClick={prevMonth} title="Previous month"
                onMouseEnter={(e) => (e.currentTarget.style.color = "#d4d4d4")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "#888")}>‹</button>
            </div>
            <span style={{ fontSize: 13, fontWeight: 600, color: "#d4d4d4" }}>
              {MONTHS[viewM]} {viewY}
            </span>
            <div style={{ display: "flex", alignItems: "center", gap: 0 }}>
              <button style={navBtnStyle} onClick={nextMonth} title="Next month"
                onMouseEnter={(e) => (e.currentTarget.style.color = "#d4d4d4")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "#888")}>›</button>
              <button style={navBtnStyle} onClick={nextYear} title="Next year"
                onMouseEnter={(e) => (e.currentTarget.style.color = "#d4d4d4")}
                onMouseLeave={(e) => (e.currentTarget.style.color = "#888")}>»</button>
            </div>
          </div>

          {/* Day headers */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", marginBottom: 4 }}>
            {DAYS.map((d) => (
              <div key={d} style={{ textAlign: "center", fontSize: 11, color: "#666", padding: "2px 0" }}>{d}</div>
            ))}
          </div>

          {/* Calendar cells */}
          <div style={{ display: "grid", gridTemplateColumns: "repeat(7, 1fr)", gap: 1 }}>
            {cells.map((d, i) => {
              if (!d) return <div key={i} />;
              const dateStr = `${viewY}-${String(viewM+1).padStart(2,"0")}-${String(d).padStart(2,"0")}`;
              const isSel = dateStr === value;
              const isToday = dateStr === todayStr;
              return (
                <button
                  key={i}
                  onMouseDown={(e) => { e.preventDefault(); selectDay(d); }}
                  style={{
                    background: isSel ? "#4a90d9" : "none",
                    border: isToday && !isSel ? "1px solid #4a90d9" : "1px solid transparent",
                    borderRadius: 3,
                    color: isSel ? "#fff" : "#d4d4d4",
                    fontSize: 12,
                    cursor: "pointer",
                    fontFamily: "inherit",
                    padding: "3px 0",
                    textAlign: "center",
                  }}
                  onMouseEnter={(e) => { if (!isSel) e.currentTarget.style.background = "#37373d"; }}
                  onMouseLeave={(e) => { if (!isSel) e.currentTarget.style.background = "none"; }}
                >
                  {d}
                </button>
              );
            })}
          </div>

          {/* Footer: Today | Clear */}
          <div style={{ display: "flex", justifyContent: "space-between", marginTop: 10, borderTop: "1px solid #3a3a3c", paddingTop: 8 }}>
            <button
              onMouseDown={(e) => {
                e.preventDefault();
                onChange(todayStr);
                setOpen(false);
              }}
              style={{ background: "none", border: "none", color: "#4a90d9", fontSize: 12, cursor: "pointer", fontFamily: "inherit", padding: "2px 4px" }}
              onMouseEnter={(e) => (e.currentTarget.style.color = "#7cb9f4")}
              onMouseLeave={(e) => (e.currentTarget.style.color = "#4a90d9")}
            >
              Today
            </button>
            <button
              onMouseDown={(e) => {
                e.preventDefault();
                onChange("");
                setOpen(false);
              }}
              style={{ background: "none", border: "none", color: "#888", fontSize: 12, cursor: "pointer", fontFamily: "inherit", padding: "2px 4px" }}
              onMouseEnter={(e) => (e.currentTarget.style.color = "#e57373")}
              onMouseLeave={(e) => (e.currentTarget.style.color = "#888")}
            >
              Clear
            </button>
          </div>
        </div>,
        document.body,
      )}
    </>
  );
}
