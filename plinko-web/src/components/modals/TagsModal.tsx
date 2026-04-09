import { useState } from "react";
import { Modal } from "../Modal";
import type { Plan, PlanRequest, PlanResponse, TagId } from "../../protocol";

interface Props {
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function TagsModal({ plan, sendRequest, onClose }: Props) {
  const [newTagName, setNewTagName] = useState("");
  const [renaming, setRenaming] = useState<Record<TagId, string>>({});

  const handleAdd = async () => {
    const name = newTagName.trim();
    if (!name) return;
    await sendRequest({ AddTag: name });
    setNewTagName("");
  };

  const handleRename = async (id: TagId) => {
    const name = (renaming[id] ?? "").trim();
    if (!name) return;
    await sendRequest({ RenameTag: [id, name] });
    setRenaming((r) => { const n = { ...r }; delete n[id]; return n; });
  };

  const handleDelete = async (id: TagId) => {
    await sendRequest({ DeleteTag: id });
  };

  return (
    <Modal title="Manage Tags" onClose={onClose} width={400}>
      <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 16 }}>
        {plan.tags.length === 0 && (
          <div style={{ color: "#666", fontSize: 13 }}>No tags yet</div>
        )}
        {plan.tags.map((tag, idx) => {
          const editVal = renaming[tag.id] ?? tag.name;
          return (
            <div
              key={tag.id}
              style={{ display: "flex", alignItems: "center", gap: 6 }}
            >
              <span style={{ color: "#555", fontSize: 11, width: 20, textAlign: "right" }}>
                {idx + 1}.
              </span>
              <input
                type="text"
                value={editVal}
                onChange={(e) =>
                  setRenaming((r) => ({ ...r, [tag.id]: e.target.value }))
                }
                onBlur={() => handleRename(tag.id)}
                onKeyDown={(e) => { if (e.key === "Enter") handleRename(tag.id); }}
                style={{
                  flex: 1,
                  background: "#1e1e1e",
                  border: "1px solid #3a3a3c",
                  borderRadius: 4,
                  color: "#d4d4d4",
                  fontSize: 13,
                  padding: "4px 8px",
                  outline: "none",
                }}
              />
              <button
                className="btn btn-danger btn-sm"
                onClick={() => handleDelete(tag.id)}
              >
                ×
              </button>
            </div>
          );
        })}
      </div>

      {/* Add new tag */}
      <div style={{ display: "flex", gap: 8 }}>
        <input
          type="text"
          placeholder="New tag name…"
          value={newTagName}
          onChange={(e) => setNewTagName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleAdd(); }}
          style={{
            flex: 1,
            background: "#1e1e1e",
            border: "1px solid #3a3a3c",
            borderRadius: 4,
            color: "#d4d4d4",
            fontSize: 13,
            padding: "6px 10px",
            outline: "none",
          }}
        />
        <button className="btn btn-primary" onClick={handleAdd} disabled={!newTagName.trim()}>
          Add
        </button>
      </div>
    </Modal>
  );
}
