/**
 * A portal-rendered dropdown picker that floats above the modal at a fixed
 * screen position anchored to a trigger element.
 */
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface PickerOption {
  key: string;
  label: string;
}

interface Props {
  anchor: HTMLElement | null;
  options: PickerOption[];
  onSelect: (key: string) => void;
  onClose: () => void;
  placeholder?: string;
}

export function FloatingPicker({ anchor, options, onSelect, onClose, placeholder = "Search…" }: Props) {
  const [filter, setFilter] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  // Position relative to anchor
  const [pos, setPos] = useState({ top: 0, left: 0, width: 200 });

  useEffect(() => {
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    // Decide whether to open below or above
    const spaceBelow = window.innerHeight - rect.bottom;
    const dropH = Math.min(options.length * 28 + 36, 220);
    const top = spaceBelow >= dropH ? rect.bottom + 2 : rect.top - dropH - 2;
    setPos({ top, left: rect.left, width: rect.width });
  }, [anchor, options.length]);

  // Auto-focus filter input
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

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
          filtered.map((o) => (
            <button
              key={o.key}
              onMouseDown={(e) => { e.preventDefault(); onSelect(o.key); }}
              style={{
                display: "block",
                width: "100%",
                textAlign: "left",
                background: "none",
                border: "none",
                padding: "6px 10px",
                color: "#d4d4d4",
                fontSize: 13,
                cursor: "pointer",
                fontFamily: "inherit",
              }}
              onMouseEnter={(e) => (e.currentTarget.style.background = "#37373d")}
              onMouseLeave={(e) => (e.currentTarget.style.background = "none")}
            >
              {o.label}
            </button>
          ))
        )}
      </div>
    </div>,
    document.body,
  );
}
