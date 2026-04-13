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
import "./App.css";

function PageRouter() {
  const { page, status, auth, logout, reconnect, plan, setPage } = usePlanContext();

  // While connecting/handshaking or auto-authenticating with a stored token
  // (auth.required is false but we haven't received PlanState yet), show a
  // blank screen to avoid the login-page flicker.
  if (status === "connecting" || status === "handshaking") {
    return <StatusScreen message="" />;
  }
  if (status === "authenticating" && !auth.required) {
    // Auto-auth in progress — blank screen with subtle fade-in
    return <StatusScreen message="" />;
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

function DisconnectedScreen({ onReconnect }: { onReconnect: () => void }) {
  return (
    <div className="status-screen">
      <div className="status-screen-content">
        <p>Disconnected from server.</p>
        <button className="btn btn-primary" onClick={onReconnect} style={{ marginTop: 16 }}>
          Reconnect
        </button>
      </div>
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
