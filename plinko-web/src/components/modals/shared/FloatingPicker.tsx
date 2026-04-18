/**
 * A portal-rendered dropdown picker that floats above the modal at a fixed
 * screen position anchored to a trigger element.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface PickerOption {
  key: string;
  label: string;
  /** Optional text color for this option (e.g. purple for Plan Start, gold for scheduler target) */
  color?: string;
}

interface Props {
  anchor: HTMLElement | null;
  options: PickerOption[];
  onSelect: (key: string) => void;
  onClose: () => void;
  placeholder?: string;
  /** Keys of currently-selected items — shown with a checkmark */
  selectedKeys?: Set<string>;
}

export function FloatingPicker({ anchor, options, onSelect, onClose, placeholder = "Search…", selectedKeys }: Props) {
  const [filter, setFilter] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Position relative to anchor — computed synchronously in a layout effect so the
  // picker appears at the correct location on the very first paint (no top-left flash).
  const [pos, setPos] = useState<{ top: number; left: number; width: number } | null>(null);

  useLayoutEffect(() => {
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    // Decide whether to open below or above
    const spaceBelow = window.innerHeight - rect.bottom;
    const dropH = Math.min(options.length * 28 + 36, 220);
    const top = spaceBelow >= dropH ? rect.bottom + 2 : rect.top - dropH - 2;
    const width = Math.max(rect.width, 200);
    const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setPos({ top, left, width });
  }, [anchor, options.length]);

  // Auto-focus filter input once position is resolved and the input is in the DOM.
  useEffect(() => {
    if (pos) inputRef.current?.focus();
  }, [pos]);

  // Close on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node) &&
          anchor && !anchor.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [anchor, onClose]);

  const filtered = filter.trim()
    ? options.filter((o) => o.label.toLowerCase().includes(filter.toLowerCase()))
    : options;

  // Don't render until position is known — useLayoutEffect guarantees this is
  // resolved before the first paint, so there is no visible flash.
  if (!pos) return null;

  return createPortal(
    <div
      ref={containerRef}
      style={{
        position: "fixed",
        top: pos.top,
        left: pos.left,
        width: Math.max(pos.width, 200),
        zIndex: 9999,
        background: "#252526",
        border: "1px solid #4a90d9",
        borderRadius: 4,
        boxShadow: "0 4px 16px rgba(0,0,0,0.6)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        maxHeight: 220,
      }}
    >
      <input
        ref={inputRef}
        type="text"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder={placeholder}
        style={{
          background: "#1e1e1e",
          border: "none",
          borderBottom: "1px solid #3a3a3c",
          color: "#d4d4d4",
          fontSize: 13,
          padding: "6px 10px",
          outline: "none",
          flexShrink: 0,
        }}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key === "Enter" && filtered.length === 1) {
            onSelect(filtered[0].key);
          }
        }}
      />
      <div style={{ overflowY: "auto", flex: 1 }}>
        {filtered.length === 0 ? (
          <div style={{ padding: "8px 10px", fontSize: 12, color: "#666" }}>No results</div>
        ) : (
          filtered.map((o) => {
            const isSel = selectedKeys?.has(o.key) ?? false;
            return (
            <button
              key={o.key}
              onMouseDown={(e) => { e.preventDefault(); onSelect(o.key); }}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                width: "100%",
                textAlign: "left",
                background: isSel ? "#1c3a5a" : "none",
                border: "none",
                padding: "6px 10px",
                color: isSel ? "#7cb9f4" : (o.color ?? "#d4d4d4"),
                fontSize: 13,
                cursor: "pointer",
                fontFamily: "inherit",
              }}
              onMouseEnter={(e) => { if (!isSel) e.currentTarget.style.background = "#37373d"; }}
              onMouseLeave={(e) => { e.currentTarget.style.background = isSel ? "#1c3a5a" : "none"; }}
            >
              <span style={{ width: 14, flexShrink: 0, color: "#4a90d9", fontSize: 11 }}>{isSel ? "✓" : ""}</span>
              {o.label}
            </button>
            );
          })
        )}
      </div>
    </div>,
    document.body,
  );
}
