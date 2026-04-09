import { useState } from "react";
import { Modal } from "../Modal";
import type { Plan } from "../../protocol";
import { displayName } from "../../utils/planUtils";

interface Props {
  plan: Plan;
  onSelect: (id: string) => void;
  onClose: () => void;
}

export function SearchModal({ plan, onSelect, onClose }: Props) {
  const [filter, setFilter] = useState("");
  const [flash, setFlash] = useState(false);
  const q = filter.toLowerCase();

  const results: { id: string; type: "task" | "milestone"; name: string; contextLabel: string | null }[] = [
    ...Object.values(plan.tasks).map((t) => ({
      id: t.id, type: "task" as const, name: t.name, contextLabel: t.context_label,
    })),
    ...Object.values(plan.milestones).map((m) => ({
      id: m.id, type: "milestone" as const, name: m.name, contextLabel: m.context_label,
    })),
  ]
    .filter((r) => !q || displayName(r.name, r.contextLabel).toLowerCase().includes(q))
    .sort((a, b) => a.name.localeCompare(b.name));

  const handleFilterChange = (val: string) => {
    setFilter(val);
    if (val && results.length === 0) {
      setFlash(true);
      setTimeout(() => setFlash(false), 600);
    }
  };

  return (
    <Modal title="Search" onClose={onClose} width={420}>
      <input
        type="text"
        value={filter}
        autoFocus
        placeholder="Filter tasks and milestones…"
        onChange={(e) => handleFilterChange(e.target.value)}
        style={{
          width: "100%",
          boxSizing: "border-box",
          background: "#1e1e1e",
          border: `1px solid ${flash ? "#e53935" : "#3a3a3c"}`,
          borderRadius: 4,
          color: "#d4d4d4",
          fontSize: 14,
          padding: "7px 10px",
          outline: "none",
          marginBottom: 10,
          transition: "border-color 0.15s",
        }}
      />
      <div style={{ maxHeight: 320, overflowY: "auto" }}>
        {results.length === 0 && (
          <div style={{ color: "#666", padding: "12px 0", textAlign: "center", fontSize: 13 }}>
            No results
          </div>
        )}
        {results.map((r) => (
          <button
            key={r.id}
            onClick={() => onSelect(r.id)}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              width: "100%",
              textAlign: "left",
              background: "none",
              border: "none",
              padding: "8px 4px",
              cursor: "pointer",
              borderBottom: "1px solid #2a2a2c",
              color: "#d4d4d4",
              fontSize: 13,
              fontFamily: "inherit",
            }}
          >
            <span style={{ color: "#888" }}>{r.type === "task" ? "▬" : "◇"}</span>
            <span>{displayName(r.name, r.contextLabel)}</span>
          </button>
        ))}
      </div>
    </Modal>
  );
}
