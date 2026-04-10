import { PlanProvider, usePlanContext } from "./context/PlanContext";
import { Toolbar } from "./components/Toolbar";
import { HomePage } from "./pages/HomePage";
import { DailyPage } from "./pages/DailyPage";
import { OverviewPage } from "./pages/OverviewPage";
import { AllocationPage } from "./pages/AllocationPage";
import { CalendarPage } from "./pages/CalendarPage";
import { SettingsPage } from "./pages/SettingsPage";
import { LoginPage } from "./pages/LoginPage";
import "./App.css";

function PageRouter() {
  const { page, status, auth } = usePlanContext();

  if (status === "connecting" || status === "handshaking") {
    return <StatusScreen message="Connecting to Plinko server…" />;
  }
  if (status === "authenticating" || auth.required) {
    return <LoginPage />;
  }
  if (status === "disconnected") {
    return <StatusScreen message="Disconnected — reconnecting…" />;
  }
  if (status === "error") {
    return <StatusScreen message="Protocol error — check server version." error />;
  }

  switch (page) {
    case "home": return <HomePage />;
    case "daily": return <DailyPage />;
    case "overview": return <OverviewPage />;
    case "allocation": return <AllocationPage />;
    case "calendar": return <CalendarPage />;
    case "settings": return <SettingsPage />;
    default: return <HomePage />;
  }
}

function StatusScreen({ message, error }: { message: string; error?: boolean }) {
  return (
    <div className={`status-screen${error ? " error" : ""}`}>
      <p>{message}</p>
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
