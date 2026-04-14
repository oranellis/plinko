import { PlanProvider, usePlanContext } from "./context/PlanContext";
import { Toolbar } from "./components/Toolbar";
import { HomePage } from "./pages/HomePage";
import { DailyPage } from "./pages/DailyPage";
import { OverviewPage } from "./pages/OverviewPage";
import { AllocationPage } from "./pages/AllocationPage";
import { ResourcesPage } from "./pages/ResourcesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { LoginPage } from "./pages/LoginPage";
import plinkoLogo from "./assets/plinko_logo.svg";
import { PROTOCOL_VERSION } from "./protocol";
import { useEffect, useState } from "react";
import type { ConnectionStatus } from "./hooks/usePlan";
import "./App.css";

function PageRouter() {
  const { page, status, auth, logout, reconnect, plan, setPage } = usePlanContext();

  if (status === "connecting" || status === "handshaking" || (status === "authenticating" && !auth.required)) {
    return <ConnectingScreen status={status} reconnect={reconnect} />;
  }
  if (status === "authenticating" || auth.required) {
    return <LoginPage />;
  }
  if (status === "disconnected") {
    return <DisconnectedScreen onReconnect={reconnect} />;
  }
  if (status === "error") {
    return <StatusScreen message="Protocol error — check server version." error />;
  }

  // No active plan — only settings is accessible.
  if (plan === null && page !== "settings") {
    return (
      <div className="no-plan-screen">
        <div className="no-plan-content">
          <img src={plinkoLogo} alt="Plinko logo" className="no-plan-logo" />
          <h1 className="no-plan-title">Plinko</h1>
          <p className="no-plan-hint">No plan active.</p>
          <button className="btn btn-primary" onClick={() => setPage("settings")}>
            Open Settings
          </button>
          {auth.currentUser && (
            <div className="no-plan-user-bar">
              <span className="no-plan-user-email">{auth.currentUser.email}</span>
              {auth.currentUser.isAdmin && (
                <span className="no-plan-user-badge">admin</span>
              )}
              <button className="no-plan-logout" onClick={logout}>Sign out</button>
            </div>
          )}
        </div>
      </div>
    );
  }

  switch (page) {
    case "home": return <HomePage />;
    case "daily": return <DailyPage />;
    case "overview": return <OverviewPage />;
    case "allocation": return <AllocationPage />;
    case "resources": return <ResourcesPage />;
    case "settings": return <SettingsPage />;
    default: return <HomePage />;
  }
}

function StatusScreen({ message, error }: { message?: string; error?: boolean }) {
  return (
    <div className={`status-screen${error ? " error" : ""}`}>
      {message && <p>{message}</p>}
    </div>
  );
}

function ConnectingScreen({ status, reconnect }: { status: ConnectionStatus; reconnect: () => void }) {
  const [elapsed, setElapsed] = useState(0);

  // Timer starts when the component mounts (i.e. when the connecting phase
  // begins) and persists across connecting → handshaking → authenticating
  // transitions because the component is never unmounted between them.
  useEffect(() => {
    const id = setInterval(() => setElapsed((s) => s + 1), 1_000);
    return () => clearInterval(id);
  }, []);

  const wsUrl = `${window.location.protocol === "https:" ? "wss" : "ws"}://${window.location.host}/ws`;

  let heading = "Connecting to server\u2026";
  if (status === "handshaking") heading = "Verifying server version\u2026";
  if (status === "authenticating") heading = "Signing in\u2026";

  const slow = elapsed >= 8;
  const verySlot = elapsed >= 15;

  return (
    <div className="status-screen connecting-screen">
      <div className="status-screen-content">
        <div className="connecting-spinner" />
        <p className="connecting-heading">{heading}</p>
        <p className="connecting-detail">
          {verySlot
            ? "Unable to reach the server. Check that it is running and try again."
            : slow
            ? "This is taking longer than expected\u2026"
            : wsUrl}
        </p>
        {elapsed >= 10 && (
          <button
            className="btn btn-secondary"
            onClick={() => { setElapsed(0); reconnect(); }}
            style={{ marginTop: 20 }}
          >
            Reconnect
          </button>
        )}
      </div>
      <span className="status-version">v{PROTOCOL_VERSION}</span>
    </div>
  );
}

function DisconnectedScreen({ onReconnect }: { onReconnect: () => void }) {
  return (
    <div className="status-screen">
      <div className="status-screen-content">
        <p>Disconnected from server.</p>
        <button className="btn btn-primary" onClick={onReconnect} style={{ marginTop: 16 }}>
          Reconnect
        </button>
      </div>
      <span className="status-version">v{PROTOCOL_VERSION}</span>
    </div>
  );
}

function RemoteUpdateToast() {
  const { remoteUpdate } = usePlanContext();
  if (!remoteUpdate) return null;
  return (
    <div className="remote-update-toast">
      Plan updated by another user
    </div>
  );
}

export default function App() {
  return (
    <PlanProvider>
      <div className="app">
        <Toolbar />
        <main className="page-area">
          <PageRouter />
        </main>
        <RemoteUpdateToast />
      </div>
    </PlanProvider>
  );
}
