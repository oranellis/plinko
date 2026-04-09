import { useState } from "react";
import { Modal } from "../Modal";
import { Plan, PlanRequest, PlanResponse, User } from "../../protocol";
import { v4 as uuidv4 } from "uuid";

interface Props {
  user: User | null;
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

export function UserFormModal({ user, plan, sendRequest, onClose }: Props) {
  const [name, setName] = useState(user?.name ?? "");
  const [selectedTags, setSelectedTags] = useState<Set<string>>(
    new Set(user?.tags ?? [])
  );
  const [saving, setSaving] = useState(false);

  const toggleTag = (tagId: string) => {
    setSelectedTags((prev) => {
      const next = new Set(prev);
      if (next.has(tagId)) next.delete(tagId);
      else next.add(tagId);
      return next;
    });
  };

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    try {
      const tags = [...selectedTags];
      if (user) {
        await sendRequest({ UpdateUser: [user.id, { name: name.trim(), tags }] });
      } else {
        await sendRequest({
          CreateUser: {
            id: uuidv4(),
            name: name.trim(),
            tags,
          },
        });
      }
      onClose();
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async () => {
    if (!user) return;
    setSaving(true);
    try {
      await sendRequest({ DeleteUser: user.id });
      onClose();
    } finally {
      setSaving(false);
    }
  };

  return (
    <Modal title={user ? `Edit ${user.name}` : "New User"} onClose={onClose} width={400}>
      <div className="form-row">
        <label>Name</label>
        <input
          type="text"
          value={name}
          autoFocus
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleSave(); }}
        />
      </div>
      {plan.tags.length > 0 && (
        <div className="form-row">
          <label>Tags</label>
          <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
            {plan.tags.map((tag) => {
              const on = selectedTags.has(tag.id);
              return (
                <button
                  key={tag.id}
                  onClick={() => toggleTag(tag.id)}
                  style={{
                    padding: "3px 10px",
                    borderRadius: 12,
                    border: "1px solid",
                    borderColor: on ? "#4a90d9" : "#3a3a3c",
                    background: on ? "#1a3a5a" : "#1e1e1e",
                    color: on ? "#4a90d9" : "#888",
                    fontSize: 12,
                    cursor: "pointer",
                    fontFamily: "inherit",
                  }}
                >
                  {tag.name}
                </button>
              );
            })}
          </div>
        </div>
      )}
      <div className="form-actions">
        {user && (
          <button className="btn btn-danger" onClick={handleDelete} disabled={saving}>
            Delete
          </button>
        )}
        <button className="btn btn-secondary" onClick={onClose}>Cancel</button>
        <button className="btn btn-primary" onClick={handleSave} disabled={saving || !name.trim()}>
          {saving ? "Saving…" : "Save"}
        </button>
      </div>
    </Modal>
  );
}
