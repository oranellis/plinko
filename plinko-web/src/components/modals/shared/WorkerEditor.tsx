/**
 * Worker editor matching the Rust UI:
 * - Bordered scrollable list (max 3 visible rows)
 * - Each row: T/P type toggle | user/tag selector button | workload input | × remove
 * - Empty state text
 * - "+" button below to add a new worker
 * - Picker dropdowns float above the modal via FloatingPicker portal
 */
import { useRef, useState } from "react";
import type { Plan, TagId, WorkerSlot } from "../../../protocol";
import { FloatingPicker } from "./FloatingPicker";
import { NumberInput } from "./NumberInput";
import type { PickerOption } from "./FloatingPicker";

interface Props {
  workers: WorkerSlot[];
  plan: Plan;
  onChange: (workers: WorkerSlot[]) => void;
}

type SlotType = "Specific" | "Placeholder";

const ROW_H = 36;
const FIXED_LIST_H = ROW_H * 3.5;

function slotType(w: WorkerSlot): SlotType {
  return "Specific" in w ? "Specific" : "Placeholder";
}

function slotWorkload(w: WorkerSlot): number {
  return "Specific" in w ? w.Specific.workload_days : w.Placeholder.workload_days;
}

export function WorkerEditor({ workers, plan, onChange }: Props) {
  const [openPickerIdx, setOpenPickerIdx] = useState<number | null>(null);
  const btnRefs = useRef<(HTMLButtonElement | null)[]>([]);

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  const userOptions: PickerOption[] = users.map((u) => ({ key: u.id, label: u.name }));
  const tagOptions: PickerOption[] = plan.tags.map((t) => ({ key: t.id, label: t.name }));

  const setType = (idx: number, type: SlotType) => {
    onChange(workers.map((w, i) => {
      if (i !== idx) return w;
      const wl = slotWorkload(w);
      if (type === "Specific") {
        // Start with empty user_id so user must pick a person
        return { Specific: { user_id: "", workload_days: wl } };
      } else {
        return { Placeholder: { required_tags: [], workload_days: wl } };
      }
    }));
  };

  const setWorkload = (idx: number, wl: number) => {
    onChange(workers.map((w, i) => {
      if (i !== idx) return w;
      if ("Specific" in w) return { Specific: { ...w.Specific, workload_days: wl } };
      return { Placeholder: { ...w.Placeholder, workload_days: wl } };
    }));
  };

  const selectUser = (idx: number, userId: string) => {
    onChange(workers.map((w, i) => {
      if (i !== idx) return w;
      const wl = slotWorkload(w);
      return { Specific: { user_id: userId, workload_days: wl } };
    }));
    setOpenPickerIdx(null);
  };

  const selectTags = (idx: number, key: string) => {
    onChange(workers.map((w, i) => {
      if (i !== idx) return w;
      if (!("Placeholder" in w)) return w;
      const wl = w.Placeholder.workload_days;
      // Toggle the tag
      const existing: TagId[] = w.Placeholder.required_tags;
      const next = existing.includes(key) ? existing.filter((t) => t !== key) : [...existing, key];
      return { Placeholder: { required_tags: next, workload_days: wl } };
    }));
    // Don't close picker — allow multi-select for tags; close via blur
  };

  const remove = (idx: number) => {
    onChange(workers.filter((_, i) => i !== idx));
    setOpenPickerIdx(null);
  };

  const addWorker = () => {
    onChange([...workers, { Placeholder: { required_tags: [], workload_days: 1 } }]);
  };

  const listH = FIXED_LIST_H;

  const getPickerLabel = (w: WorkerSlot): string => {
    if ("Specific" in w) {
      const u = plan.users_data[w.Specific.user_id]?.user;
      return u?.name ?? "Select person…";
    }
    if (w.Placeholder.required_tags.length === 0) return "Any (no tags required)";
    const names = w.Placeholder.required_tags
      .map((id) => plan.tags.find((t) => t.id === id)?.name ?? id)
      .sort();
    return names.join(", ");
  };

  const isPickerLabelMuted = (w: WorkerSlot): boolean => {
    if ("Specific" in w) return !w.Specific.user_id || !plan.users_data[w.Specific.user_id];
    return w.Placeholder.required_tags.length === 0;
  };

  return (
    <div className="form-row">
      <label>Workers</label>

      {/* Bordered list */}
      <div style={{
        border: "1px solid #3a3a3c",
        borderRadius: 4,
        height: listH,
        overflowY: "auto",
        background: "#1e1e1e",
        position: "relative",
      }}>
        {workers.length === 0 ? (
          <div style={{ padding: "8px 12px", fontSize: 13, color: "#555" }}>
            No workers added yet
          </div>
        ) : (
          workers.map((w, idx) => {
            const type = slotType(w);
            const wl = slotWorkload(w);
            const pickerLabel = getPickerLabel(w);
            const muted = isPickerLabelMuted(w);
            const pickerOpen = openPickerIdx === idx;
            const opts = type === "Specific" ? userOptions : tagOptions;

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
                  {/* T/P type toggle — T (tags/Placeholder) first, P (person/Specific) second */}
                  <div style={{
                    display: "flex",
                    border: "1px solid #3a3a3c",
                    borderRadius: 4,
                    overflow: "hidden",
                    flexShrink: 0,
                  }}>
                    {(["Placeholder", "Specific"] as SlotType[]).map((t, ti) => (
                      <button
                        key={t}
                        onClick={() => setType(idx, t)}
                        title={t === "Specific" ? "Specific person" : "Placeholder (by tags)"}
                        style={{
                          background: type === t ? "#4a90d9" : "none",
                          border: "none",
                          color: type === t ? "#fff" : "#999",
                          fontSize: 11,
                          fontWeight: 600,
                          padding: "2px 7px",
                          cursor: "pointer",
                          fontFamily: "inherit",
                          borderRight: ti === 0 ? "1px solid #3a3a3c" : "none",
                        }}
                      >
                        {t === "Specific" ? "P" : "T"}
                      </button>
                    ))}
                  </div>

                  {/* User/tag selector */}
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
                      color: muted ? "#555" : "#d4d4d4",
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
                      {pickerLabel}
                    </span>
                    <span style={{ color: "#666", fontSize: 10, flexShrink: 0, marginLeft: 4 }}>▾</span>
                  </button>

                  {/* Workload input */}
                  <NumberInput
                    min={0}
                    step={0.5}
                    value={wl}
                    title="Workload (days)"
                    onChange={(v) => setWorkload(idx, v)}
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
                  <span style={{ fontSize: 11, color: "#555", flexShrink: 0 }}>d</span>

                  {/* Remove */}
                  <button
                    onClick={() => remove(idx)}
                    title="Remove worker"
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

                  {/* Floating picker for this row */}
                  {pickerOpen && (
                    <FloatingPicker
                      anchor={btnRefs.current[idx]}
                      options={opts}
                      onSelect={(key) => {
                        if (type === "Specific") selectUser(idx, key);
                        else selectTags(idx, key);
                      }}
                      onClose={() => setOpenPickerIdx(null)}
                      placeholder={type === "Specific" ? "Search person…" : "Search tags…"}
                      selectedKeys={type === "Placeholder" && "Placeholder" in w
                        ? new Set(w.Placeholder.required_tags)
                        : undefined}
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
        onClick={addWorker}
        style={{
          alignSelf: "flex-start",
          fontSize: 13,
          padding: "5px 12px",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <span style={{ fontSize: 16, lineHeight: 1 }}>+</span> Add Worker
      </button>
    </div>
  );
}
