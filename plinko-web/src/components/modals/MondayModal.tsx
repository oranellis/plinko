import { useEffect, useState } from "react";
import { Modal } from "../Modal";
import {
  BoardColumn,
  MondayConfig,
  MondayUser,
  Plan,
  PlanRequest,
  PlanResponse,
  Status,
  UserId,
} from "../../protocol";
import { usePlanContext } from "../../context/PlanContext";

interface Props {
  planId: string;
  onClose: () => void;
}

const DEFAULT_CONFIG: MondayConfig = {
  board_id: "",
  column_map: {
    person_column_id: "",
    status_column_id: "",
    dependency_column_id: "",
    workload_column_id: "",
    timeline_column_id: "",
  },
  user_mappings: [],
  status_mappings: [],
  item_node_map: [],
  use_subitems: false,
  workload_in_hours: false,
  show_monday_context: false,
};

const PLINKO_STATUSES: Status[] = [
  "NotStarted", "InProgress", "OnHold", "Complete", "Dropped",
];

export function MondayModal({ planId, onClose }: Props) {
  const { plan, sendRequest, monday } = usePlanContext();
  const [token, setToken] = useState("");
  const [config, setConfig] = useState<MondayConfig>(DEFAULT_CONFIG);
  const [boardUsers, setBoardUsers] = useState<MondayUser[]>([]);
  const [statusLabels, setStatusLabels] = useState<string[]>([]);
  const [testResult, setTestResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // Load existing config on mount
  useEffect(() => {
    sendRequest({ LoadMondayConfig: { plan_id: planId } }).then((resp) => {
      if (typeof resp === "object" && "MondayConfigLoaded" in resp) {
        setConfig(resp.MondayConfigLoaded);
      }
    });
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [planId]);

  const handleTestConnection = async () => {
    setTestResult(null);
    setLoading(true);
    try {
      const resp = await sendRequest({
        MondayTestConnection: { token, board_id: config.board_id },
      });
      if (typeof resp === "object" && "Error" in resp) {
        setTestResult("❌ " + JSON.stringify(resp.Error));
      } else {
        setTestResult("✓ Connected");
      }
    } catch (e) {
      setTestResult("❌ " + String(e));
    } finally {
      setLoading(false);
    }
  };

  const handleFetchBoardInfo = async () => {
    setLoading(true);
    try {
      const resp = await sendRequest({
        MondayFetchBoardInfo: { token, board_id: config.board_id },
      });
      if (typeof resp === "object" && "MondayBoardInfo" in resp) {
        const info = resp.MondayBoardInfo;
        setBoardUsers(info.users);
        setStatusLabels(info.status_labels);
        // Auto-populate user mappings for new users
        setConfig((c) => {
          const existing = new Set(c.user_mappings.map((m) => m.monday_user_id));
          const newMappings = info.users
            .filter((u) => !existing.has(u.id))
            .map((u) => ({ monday_user_id: u.id, monday_name: u.name, plinko_user_id: null }));
          const newStatusMappings = info.status_labels
            .filter((l) => !c.status_mappings.find((m) => m.monday_label === l))
            .map((l) => ({ monday_label: l, plinko_status: "NotStarted" as Status }));
          return {
            ...c,
            user_mappings: [...c.user_mappings, ...newMappings],
            status_mappings: [...c.status_mappings, ...newStatusMappings],
          };
        });
      }
    } finally {
      setLoading(false);
    }
  };

  const handleSaveConfig = async () => {
    await sendRequest({ SaveMondayConfig: { plan_id: planId, config, token } });
  };

  const handlePull = async () => {
    await handleSaveConfig();
    await sendRequest({ MondayPull: { plan_id: planId } });
  };

  const handleFullReimport = async () => {
    await handleSaveConfig();
    await sendRequest({ MondayFullReimport: { plan_id: planId } });
  };

  const handlePush = async () => {
    await handleSaveConfig();
    await sendRequest({ MondayPush: { plan_id: planId } });
  };

  const planUsers = plan
    ? Object.values(plan.users_data)
        .map((ud) => ud.user)
        .sort((a, b) => a.name.localeCompare(b.name))
    : [];

  const setUserMapping = (mondayUserId: string, plinkoUserId: UserId | null) => {
    setConfig((c) => ({
      ...c,
      user_mappings: c.user_mappings.map((m) =>
        m.monday_user_id === mondayUserId ? { ...m, plinko_user_id: plinkoUserId } : m
      ),
    }));
  };

  const setStatusMapping = (mondayLabel: string, plinkoStatus: Status) => {
    setConfig((c) => ({
      ...c,
      status_mappings: c.status_mappings.map((m) =>
        m.monday_label === mondayLabel ? { ...m, plinko_status: plinkoStatus } : m
      ),
    }));
  };

  const progressing = monday.progress !== null;

  return (
    <Modal title="Monday.com Integration" onClose={onClose} width={520}>
      <div style={{ display: "flex", flexDirection: "column", gap: 20 }}>
        {/* Connection */}
        <section>
          <h3 style={sectionTitle}>Connection</h3>
          <div className="form-row">
            <label>API Token</label>
            <input
              type="text"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="xxxx-xxxx…"
            />
          </div>
          <div className="form-row">
            <label>Board ID</label>
            <input
              type="text"
              value={config.board_id}
              onChange={(e) => setConfig((c) => ({ ...c, board_id: e.target.value }))}
            />
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
            <button
              className="btn btn-secondary btn-sm"
              onClick={handleTestConnection}
              disabled={loading}
            >
              Test Connection
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={handleFetchBoardInfo}
              disabled={loading}
            >
              Fetch Board Info
            </button>
            {testResult && (
              <span style={{ fontSize: 12, color: testResult.startsWith("✓") ? "#4caf50" : "#e57373" }}>
                {testResult}
              </span>
            )}
          </div>
        </section>

        {/* Column mapping */}
        <section>
          <h3 style={sectionTitle}>Column Mapping</h3>
          {(["person_column_id", "status_column_id", "dependency_column_id", "workload_column_id", "timeline_column_id"] as const).map((k) => (
            <div className="form-row" key={k}>
              <label>{k.replace(/_column_id$/, "").replace(/_/g, " ")}</label>
              <input
                type="text"
                value={config.column_map[k]}
                onChange={(e) =>
                  setConfig((c) => ({ ...c, column_map: { ...c.column_map, [k]: e.target.value } }))
                }
              />
            </div>
          ))}
        </section>

        {/* Item type */}
        <section>
          <h3 style={sectionTitle}>Item Type</h3>
          <div style={{ display: "flex", gap: 16 }}>
            {[{ label: "Subitems", val: true }, { label: "Items", val: false }].map(({ label, val }) => (
              <label key={label} style={{ display: "flex", gap: 6, alignItems: "center", cursor: "pointer", color: "#d4d4d4", fontSize: 13 }}>
                <input
                  type="radio"
                  name="item-type"
                  checked={config.use_subitems === val}
                  onChange={() => setConfig((c) => ({ ...c, use_subitems: val }))}
                />
                {label}
              </label>
            ))}
          </div>
        </section>

        {/* Workload unit */}
        <section>
          <h3 style={sectionTitle}>Workload Unit</h3>
          <div style={{ display: "flex", gap: 16 }}>
            {[{ label: "Days", val: false }, { label: "Hours", val: true }].map(({ label, val }) => (
              <label key={label} style={{ display: "flex", gap: 6, alignItems: "center", cursor: "pointer", color: "#d4d4d4", fontSize: 13 }}>
                <input
                  type="radio"
                  name="workload-unit"
                  checked={config.workload_in_hours === val}
                  onChange={() => setConfig((c) => ({ ...c, workload_in_hours: val }))}
                />
                {label}
              </label>
            ))}
          </div>
        </section>

        {/* Show context */}
        <section>
          <h3 style={sectionTitle}>Show Group/Parent Context</h3>
          <div style={{ display: "flex", gap: 16 }}>
            {[{ label: "On", val: true }, { label: "Off", val: false }].map(({ label, val }) => (
              <label key={label} style={{ display: "flex", gap: 6, alignItems: "center", cursor: "pointer", color: "#d4d4d4", fontSize: 13 }}>
                <input
                  type="radio"
                  name="show-context"
                  checked={config.show_monday_context === val}
                  onChange={() => setConfig((c) => ({ ...c, show_monday_context: val }))}
                />
                {label}
              </label>
            ))}
          </div>
        </section>

        {/* User mappings */}
        {config.user_mappings.length > 0 && (
          <section>
            <h3 style={sectionTitle}>User Mappings</h3>
            {config.user_mappings.map((m) => (
              <div key={m.monday_user_id} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
                <span style={{ flex: 1, fontSize: 12, color: "#d4d4d4" }}>{m.monday_name}</span>
                <span style={{ color: "#666", fontSize: 12 }}>→</span>
                <select
                  value={m.plinko_user_id ?? ""}
                  onChange={(e) => setUserMapping(m.monday_user_id, e.target.value || null)}
                  style={selectStyle}
                >
                  <option value="">Unassigned</option>
                  {planUsers.map((u) => (
                    <option key={u.id} value={u.id}>{u.name}</option>
                  ))}
                </select>
              </div>
            ))}
          </section>
        )}

        {/* Status mappings */}
        {config.status_mappings.length > 0 && (
          <section>
            <h3 style={sectionTitle}>Status Mappings</h3>
            {config.status_mappings.map((m) => (
              <div key={m.monday_label} style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 6 }}>
                <span style={{ flex: 1, fontSize: 12, color: "#d4d4d4" }}>{m.monday_label}</span>
                <span style={{ color: "#666", fontSize: 12 }}>→</span>
                <select
                  value={m.plinko_status}
                  onChange={(e) => setStatusMapping(m.monday_label, e.target.value as Status)}
                  style={selectStyle}
                >
                  {PLINKO_STATUSES.map((s) => (
                    <option key={s} value={s}>{s}</option>
                  ))}
                </select>
              </div>
            ))}
          </section>
        )}

        {/* Sync actions */}
        <section>
          <h3 style={sectionTitle}>Sync</h3>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 8 }}>
            <button className="btn btn-primary btn-sm" onClick={handlePull} disabled={progressing}>
              Pull from Monday
            </button>
            <button className="btn btn-secondary btn-sm" onClick={handleFullReimport} disabled={progressing}>
              Full Re-import
            </button>
            <button className="btn btn-secondary btn-sm" onClick={handlePush} disabled={progressing}>
              Push dates to Monday
            </button>
          </div>
          {progressing && monday.progress && (
            <div style={{ fontSize: 12, color: "#4a90d9" }}>
              {monday.progress.message} ({monday.progress.done}/{monday.progress.total})
            </div>
          )}
          {monday.lastMessage && !progressing && (
            <div style={{ fontSize: 12, color: "#4caf50" }}>{monday.lastMessage}</div>
          )}
          {monday.lastError && (
            <div style={{ fontSize: 12, color: "#e57373" }}>{monday.lastError}</div>
          )}
        </section>

        <div className="form-actions">
          <button className="btn btn-secondary" onClick={onClose}>Close</button>
          <button className="btn btn-primary" onClick={handleSaveConfig}>Save Config</button>
        </div>
      </div>
    </Modal>
  );
}

const sectionTitle: React.CSSProperties = {
  fontSize: 12,
  fontWeight: 600,
  color: "#888",
  textTransform: "uppercase",
  letterSpacing: "0.06em",
  margin: "0 0 10px 0",
};

const selectStyle: React.CSSProperties = {
  background: "#1e1e1e",
  border: "1px solid #3a3a3c",
  borderRadius: 4,
  color: "#d4d4d4",
  fontSize: 12,
  padding: "3px 6px",
  outline: "none",
};
