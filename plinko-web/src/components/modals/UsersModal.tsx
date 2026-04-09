import { useState } from "react";
import { Modal } from "../Modal";
import type { Plan, PlanRequest, PlanResponse, User, UserId } from "../../protocol";
import { UserFormModal } from "./UserFormModal";
import { ScheduleModal } from "./ScheduleModal";
import { TagsModal } from "./TagsModal";

interface Props {
  plan: Plan;
  sendRequest: (r: PlanRequest) => Promise<PlanResponse>;
  onClose: () => void;
}

type Sub = { type: "userForm"; userId: UserId | null } | { type: "schedule"; userId: UserId } | { type: "tags" };

export function UsersModal({ plan, sendRequest, onClose }: Props) {
  const [sub, setSub] = useState<Sub | null>(null);

  if (sub?.type === "userForm") {
    const user = sub.userId ? plan.users_data[sub.userId]?.user ?? null : null;
    return (
      <UserFormModal
        user={user}
        plan={plan}
        sendRequest={sendRequest}
        onClose={() => setSub(null)}
      />
    );
  }
  if (sub?.type === "schedule") {
    return (
      <ScheduleModal
        userId={sub.userId}
        plan={plan}
        sendRequest={sendRequest}
        onClose={() => setSub(null)}
      />
    );
  }
  if (sub?.type === "tags") {
    return (
      <TagsModal
        plan={plan}
        sendRequest={sendRequest}
        onClose={() => setSub(null)}
      />
    );
  }

  const users = Object.values(plan.users_data)
    .map((ud) => ud.user)
    .sort((a, b) => a.name.localeCompare(b.name));

  const handleDelete = async (userId: UserId) => {
    await sendRequest({ DeleteUser: userId });
  };

  return (
    <Modal title="Users" onClose={onClose} width={480}>
      <div style={{ marginBottom: 12, display: "flex", gap: 8 }}>
        <button
          className="btn btn-primary"
          onClick={() => setSub({ type: "userForm", userId: null })}
        >
          + Add User
        </button>
        <button
          className="btn btn-secondary"
          onClick={() => setSub({ type: "tags" })}
        >
          Manage Tags
        </button>
      </div>

      <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
        {users.length === 0 && (
          <div style={{ color: "#666", fontSize: 13, padding: "10px 0" }}>No users</div>
        )}
        {users.map((u: User) => (
          <div
            key={u.id}
            style={{
              display: "flex",
              alignItems: "center",
              padding: "8px 10px",
              background: "#1e1e1e",
              borderRadius: 4,
              gap: 8,
            }}
          >
            <span style={{ flex: 1, fontSize: 14, color: "#d4d4d4" }}>{u.name}</span>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => setSub({ type: "userForm", userId: u.id })}
            >
              Edit
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => setSub({ type: "schedule", userId: u.id })}
            >
              Schedule
            </button>
            <button
              className="btn btn-danger btn-sm"
              onClick={() => handleDelete(u.id)}
            >
              Delete
            </button>
          </div>
        ))}
      </div>
    </Modal>
  );
}
