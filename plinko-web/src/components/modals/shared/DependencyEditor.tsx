/**
 * Dependency / "Required by" list editor matching the Rust UI:
 * - Bordered scrollable list (max 3 visible rows)
 * - Each row: target selector button (portal dropdown) | lag input | × remove
 * - Row separators between entries
 * - Empty state text
 * - "+" button below to add
 * - When mode="dependents", excludes PlanStart from options and only allows Task/Milestone nodes
 */
import { useLayoutEffect, useRef, useState } from "react";
import type { Dependency, NodeId, Plan } from "../../../protocol";
import { FloatingPicker } from "./FloatingPicker";
import { NumberInput } from "./NumberInput";
import type { PickerOption } from "./FloatingPicker";

interface Props {
  label: string;
  deps: Dependency[];
  plan: Plan;
  /** Node to exclude from options (the node being edited) */
  excludeNodeId: NodeId | null;
  /** Additional node IDs to exclude (as stringified keys) */
  excludeKeys?: Set<string>;
  /** When true, do not offer PlanStart as an option */
  noPlanStart?: boolean;
  onChange: (deps: Dependency[]) => void;
  /** Placeholder text for the target button when unset */
  emptyLabel?: string;
  /** Text shown when list is empty */
  emptyStateText?: string;
  error?: boolean;
}

interface NodeOption extends PickerOption {
  nodeId: NodeId;
}

const ROW_H = 36;
const FIXED_LIST_H = ROW_H * 3.5;

function nodeKey(n: NodeId): string {
  if (n === "PlanStart") return "PlanStart";
  if (typeof n === "object" && "Task" in n) return `task:${n.Task}`;
  if (typeof n === "object" && "Milestone" in n) return `milestone:${n.Milestone}`;
  return String(n);
}

function buildOptions(plan: Plan, exclude: NodeId | null, excludeKeys: Set<string>, noPlanStart: boolean): NodeOption[] {
  const excludeK = exclude ? nodeKey(exclude) : null;
  const opts: NodeOption[] = [];
  if (!noPlanStart) {
    opts.push({ key: "PlanStart", label: "Plan Start", nodeId: "PlanStart" });
  }
  for (const t of Object.values(plan.tasks)) {
    const k = `task:${t.id}`;
    if (k !== excludeK && !excludeKeys.has(k)) {
      opts.push({ key: k, label: t.name, nodeId: { Task: t.id } });
    }
  }
  for (const m of Object.values(plan.milestones)) {
    const k = `milestone:${m.id}`;
    if (k !== excludeK && !excludeKeys.has(k)) {
      opts.push({ key: k, label: m.name, nodeId: { Milestone: m.id } });
    }
  }
  return opts.sort((a, b) => a.label.localeCompare(b.label));
}

function nodeLabel(id: NodeId, plan: Plan): string {
  if (id === "PlanStart") return "Plan Start";
  if (typeof id === "object" && "Task" in id) return plan.tasks[id.Task]?.name ?? id.Task;
  if (typeof id === "object" && "Milestone" in id) return plan.milestones[id.Milestone]?.name ?? id.Milestone;
  return String(id);
}

export function DependencyEditor({
  label,
  deps,
  plan,
  excludeNodeId,
  excludeKeys = new Set(),
  noPlanStart = false,
  onChange,
  emptyLabel = "Select dependency…",
  emptyStateText = "No dependencies added yet",
  error = false,
}: Props) {
  const [openPickerIdx, setOpenPickerIdx] = useState<number | null>(null);
  const btnRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // When a new dep is added, we want to open the picker for it — but the button
  // ref isn't available until after the DOM is committed. Store the pending index
  // and open it in a layout effect once the ref exists.
  const pendingOpenRef = useRef<number | null>(null);

  // eslint-disable-next-line react-hooks/exhaustive-deps -- intentionally runs every render to detect when the btn ref is ready
  useLayoutEffect(() => {
    if (pendingOpenRef.current !== null) {
      const idx = pendingOpenRef.current;
      if (btnRefs.current[idx]) {
        pendingOpenRef.current = null;
        setOpenPickerIdx(idx);
      }
    }
  });

  const allOptions = buildOptions(plan, excludeNodeId, excludeKeys, noPlanStart);

  const updateDep = (idx: number, dep: Dependency) => {
    const next = [...deps];
    next[idx] = dep;
    onChange(next);
  };

  const removeDep = (idx: number) => {
    onChange(deps.filter((_, i) => i !== idx));
    setOpenPickerIdx(null);
  };

  const addDep = () => {
    const newIdx = deps.length;
    onChange([...deps, { id: allOptions[0]?.nodeId ?? "PlanStart", lag_days: 0 }]);
    // Don't call setOpenPickerIdx here — the new button's ref isn't in the DOM yet.
    // The useLayoutEffect above will open it once the ref is available.
    pendingOpenRef.current = newIdx;
  };

  const selectTarget = (idx: number, key: string) => {
    const opt = allOptions.find((o) => o.key === key);
    if (!opt) return;
    updateDep(idx, { ...deps[idx], id: opt.nodeId });
    setOpenPickerIdx(null);
  };

  const listH = FIXED_LIST_H;
  const borderColor = error ? "#e53935" : "#3a3a3c";

  return (
    <div className="form-row">
      <label style={{ color: error ? "#e57373" : undefined }}>{label}</label>

      {/* Bordered list */}
      <div style={{
        border: `1px solid ${borderColor}`,
        borderRadius: 4,
        height: listH,
        overflowY: "auto",
        background: "#1e1e1e",
      }}>
        {deps.length === 0 ? (
          <div style={{ padding: "8px 12px", fontSize: 13, color: "#555" }}>
            {emptyStateText}
          </div>
        ) : (
          deps.map((dep, idx) => {
            const pickerOpen = openPickerIdx === idx;
            const targetSet = dep.id !== null && dep.id !== undefined;
            const label = targetSet ? nodeLabel(dep.id, plan) : "";
            const isPlanStart = dep.id === "PlanStart";

            return (
              <div key={idx}>
                {idx > 0 && (
                  <div style={{ height: 1, background: "#2d2d30" }} />
                )}
                <div style={{
                  display: "flex",
                  alignItems: "center",
                  height: ROW_H,
                  padding: "0 8px",
                  gap: 6,
                }}>
                  {/* Target selector button */}
                  <button
                    ref={(el) => { btnRefs.current[idx] = el; }}
                    onClick={() => setOpenPickerIdx(pickerOpen ? null : idx)}
                    style={{
                      flex: 1,
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "space-between",
                      background: "#252526",
                      border: `1px solid ${pickerOpen ? "#4a90d9" : "#3a3a3c"}`,
                      borderRadius: 4,
                      color: !label ? "#555" : isPlanStart ? "#a78bfa" : "#d4d4d4",
                      fontSize: 13,
                      padding: "0 8px",
                      height: 26,
                      cursor: "pointer",
                      fontFamily: "inherit",
                      textAlign: "left",
                      minWidth: 0,
                      overflow: "hidden",
                    }}
                  >
                    <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {label || emptyLabel}
                    </span>
                    <span style={{ color: "#666", fontSize: 10, flexShrink: 0, marginLeft: 4 }}>▾</span>
                  </button>

                  {/* Lag input */}
                  <NumberInput
                    value={dep.lag_days}
                    step={0.5}
                    title="Lag days (positive = delay, negative = lead)"
                    onChange={(v) => updateDep(idx, { ...dep, lag_days: v })}
                    style={{
                      width: 56,
                      flexShrink: 0,
                      background: "#252526",
                      border: "1px solid #3a3a3c",
                      borderRadius: 4,
                      color: "#d4d4d4",
                      fontSize: 13,
                      padding: "3px 6px",
                      outline: "none",
                      textAlign: "center",
                    }}
                  />
                  <span style={{ fontSize: 11, color: "#555", flexShrink: 0 }}>lag</span>

                  {/* Remove */}
                  <button
                    onClick={() => removeDep(idx)}
                    title="Remove"
                    style={{
                      background: "none",
                      border: "none",
                      color: "#555",
                      cursor: "pointer",
                      fontSize: 20,
                      lineHeight: 1,
                      padding: "4px 8px",
                      flexShrink: 0,
                    }}
                    onMouseEnter={(e) => (e.currentTarget.style.color = "#e57373")}
                    onMouseLeave={(e) => (e.currentTarget.style.color = "#555")}
                  >
                    ×
                  </button>

                  {/* Floating picker */}
                  {pickerOpen && (
                    <FloatingPicker
                      anchor={btnRefs.current[idx]}
                      options={allOptions}
                      onSelect={(key) => selectTarget(idx, key)}
                      onClose={() => setOpenPickerIdx(null)}
                    />
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>

      {/* + Add button */}
      <button
        className="btn btn-secondary"
        onClick={addDep}
        disabled={allOptions.length === 0}
        style={{
          alignSelf: "flex-start",
          fontSize: 13,
          padding: "5px 12px",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <span style={{ fontSize: 16, lineHeight: 1 }}>+</span> Add
      </button>
    </div>
  );
}

