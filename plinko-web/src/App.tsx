import { PlanProvider, usePlanContext } from "./context/PlanContext";
import { Toolbar } from "./components/Toolbar";
import { HomePage } from "./pages/HomePage";
import { DailyPage } from "./pages/DailyPage";
import { OverviewPage } from "./pages/OverviewPage";
import { AllocationPage } from "./pages/AllocationPage";
import { ResourcesPage } from "./pages/ResourcesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { LoginPage } from "./pages/LoginPage";
import "./App.css";

function PageRouter() {
  const { page, status, auth, reconnect } = usePlanContext();

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

export default function App() {
  return (
    <PlanProvider>
      <div className="app">
        <Toolbar />
        <main className="page-area">
          <PageRouter />
        </main>
      </div>
    </PlanProvider>
  );
}
