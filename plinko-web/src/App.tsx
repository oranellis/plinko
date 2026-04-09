import { usePlan } from "./hooks/usePlan";
import { Plan } from "./protocol";
import "./App.css";

export default function App() {
  const { plan, status, sendRequest } = usePlan();

  return (
    <div className="app">
      <Toolbar plan={plan} status={status} />
      <main className="page-area">
        {status === "connecting" || status === "handshaking" ? (
          <StatusScreen message="Connecting to Plinko server…" />
        ) : status === "disconnected" ? (
          <StatusScreen message="Disconnected — reconnecting…" />
        ) : status === "error" ? (
          <StatusScreen message="Protocol error — check server version." error />
        ) : plan ? (
          <HomePage plan={plan} sendRequest={sendRequest} />
        ) : (
          <StatusScreen message="Waiting for plan data…" />
        )}
      </main>
    </div>
  );
}

// ── Placeholder components ────────────────────────────────────────────────────

function Toolbar({ plan, status }: { plan: Plan | null; status: string }) {
  const dot =
    status === "connected" ? "🟢" : status === "connecting" || status === "handshaking" ? "🟡" : "🔴";
  return (
    <header className="toolbar">
      <span className="toolbar-title">Plinko</span>
      {plan && <span className="toolbar-plan-name">{plan.name}</span>}
      <span className="toolbar-status" title={status}>
        {dot}
      </span>
    </header>
  );
}

function StatusScreen({ message, error }: { message: string; error?: boolean }) {
  return (
    <div className={`status-screen${error ? " error" : ""}`}>
      <p>{message}</p>
    </div>
  );
}

function HomePage({
  plan,
  sendRequest,
}: {
  plan: Plan;
  sendRequest: ReturnType<typeof usePlan>["sendRequest"];
}) {
  const taskCount = Object.keys(plan.tasks).length;
  const milestoneCount = Object.keys(plan.milestones).length;
  const userCount = Object.keys(plan.users_data).length;

  return (
    <div className="home-page">
      <h1>{plan.name}</h1>
      <div className="summary-cards">
        <SummaryCard label="Tasks" value={taskCount} />
        <SummaryCard label="Milestones" value={milestoneCount} />
        <SummaryCard label="Team members" value={userCount} />
      </div>
      <p className="home-hint">
        Full page navigation coming soon. More pages will appear here as the migration progresses.
      </p>
      <button
        className="btn"
        onClick={() => sendRequest("RunScheduler").catch(console.error)}
      >
        Run Scheduler
      </button>
    </div>
  );
}

function SummaryCard({ label, value }: { label: string; value: number }) {
  return (
    <div className="summary-card">
      <span className="summary-value">{value}</span>
      <span className="summary-label">{label}</span>
    </div>
  );
}
