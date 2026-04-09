import { useState } from "react";
import { Dependency, NodeId, Plan } from "../../../protocol";

interface Props {
  label: string;
  deps: Dependency[];
  plan: Plan;
  excludeNodeId: NodeId | null;
  onChange: (deps: Dependency[]) => void;
}

interface NodeOption {
  key: string;
  label: string;
  nodeId: NodeId;
}

function buildNodeOptions(plan: Plan, exclude: NodeId | null): NodeOption[] {
  const excludeKey = exclude ? nodeKey(exclude) : null;
  const opts: NodeOption[] = [
    { key: "PlanStart", label: "Plan Start", nodeId: "PlanStart" },
    ...Object.values(plan.tasks).map((t) => ({
      key: `task:${t.id}`,
      label: t.name,
      nodeId: { Task: t.id } as NodeId,
    })),
    ...Object.values(plan.milestones).map((m) => ({
      key: `milestone:${m.id}`,
      label: m.name,
      nodeId: { Milestone: m.id } as NodeId,
    })),
  ];
  return opts.filter((o) => o.key !== excludeKey).sort((a, b) => a.label.localeCompare(b.label));
}

function nodeKey(n: NodeId): string {
  if (n === "PlanStart") return "PlanStart";
  if ("Task" in n) return `task:${n.Task}`;
  return `milestone:${n.Milestone}`;
}

export function DependencyEditor({ label, deps, plan, excludeNodeId, onChange }: Props) {
  const [filter, setFilter] = useState("");
  const [activeIdx, setActiveIdx] = useState<number | null>(null);
  const options = buildNodeOptions(plan, excludeNodeId);

  const updateDep = (idx: number, dep: Dependency) => {
    const next = [...deps];
    next[idx] = dep;
    onChange(next);
  };

  const removeDep = (idx: number) => {
    onChange(deps.filter((_, i) => i !== idx));
  };

  const addDep = (nodeId: NodeId) => {
    onChange([...deps, { id: nodeId, lag_days: 0 }]);
    setActiveIdx(null);
    setFilter("");
  };

  const filteredOpts = options.filter((o) => {
    if (!filter) return true;
    return o.label.toLowerCase().includes(filter.toLowerCase());
  });

  return (
    <div className="form-row">
      <label>{label}</label>
      <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        {deps.map((dep, idx) => {
          const opt = options.find((o) => nodeKey(o.nodeId) === nodeKey(dep.id));
          return (
            <div key={idx} style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <span style={{ flex: 1, fontSize: 12, color: "#d4d4d4" }}>
                {opt?.label ?? nodeKey(dep.id)}
              </span>
              <input
                type="number"
                step={0.5}
                value={dep.lag_days}
                onChange={(e) =>
                  updateDep(idx, { ...dep, lag_days: parseFloat(e.target.value) || 0 })
                }
                title="Lag days"
                style={{
                  width: 60,
                  background: "#1e1e1e",
                  border: "1px solid #3a3a3c",
                  borderRadius: 4,
                  color: "#d4d4d4",
                  fontSize: 12,
                  padding: "3px 6px",
                  outline: "none",
                }}
              />
              <span style={{ fontSize: 11, color: "#666" }}>lag</span>
              <button
                onClick={() => removeDep(idx)}
                style={{
                  background: "none", border: "none", color: "#888", cursor: "pointer", fontSize: 14
                }}
              >
                ×
              </button>
            </div>
          );
        })}

        {/* Picker */}
        {activeIdx === -1 ? (
          <div style={{ position: "relative" }}>
            <input
              type="text"
              value={filter}
              autoFocus
              placeholder="Search…"
              onChange={(e) => setFilter(e.target.value)}
              onBlur={() => setTimeout(() => setActiveIdx(null), 150)}
              style={{
                width: "100%",
                boxSizing: "border-box",
                background: "#1e1e1e",
                border: "1px solid #4a90d9",
                borderRadius: 4,
                color: "#d4d4d4",
                fontSize: 12,
                padding: "4px 8px",
                outline: "none",
              }}
            />
            {filteredOpts.length > 0 && (
              <div
                style={{
                  position: "absolute",
                  top: "100%",
                  left: 0,
                  right: 0,
                  background: "#252526",
                  border: "1px solid #3a3a3c",
                  borderRadius: 4,
                  maxHeight: 160,
                  overflowY: "auto",
                  zIndex: 100,
                }}
              >
                {filteredOpts.map((o) => (
                  <button
                    key={o.key}
                    onMouseDown={() => addDep(o.nodeId)}
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      background: "none",
                      border: "none",
                      padding: "6px 10px",
                      color: "#d4d4d4",
                      fontSize: 12,
                      cursor: "pointer",
                      fontFamily: "inherit",
                    }}
                  >
                    {o.label}
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          <button
            className="btn btn-secondary btn-sm"
            onClick={() => setActiveIdx(-1)}
            style={{ alignSelf: "flex-start" }}
          >
            + Add
          </button>
        )}
      </div>
    </div>
  );
}
