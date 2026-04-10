import type { ReactNode } from "react";
import { type PageId, usePlanContext } from "../context/PlanContext";
import {
  IconGantt,
  IconAllocation,
  IconCalendar,
} from "../components/icons";
import "./HomePage.css";

interface NavCard {
  id: PageId;
  label: string;
  icon: ReactNode;
}

const CARDS: NavCard[] = [
  { id: "overview",   label: "Overview",   icon: <IconGantt size={36} /> },
  { id: "allocation", label: "Allocation", icon: <IconAllocation size={36} /> },
  { id: "calendar",   label: "Calendar",   icon: <IconCalendar size={36} /> },
];

export function HomePage() {
  const { setPage, auth, logout } = usePlanContext();

  return (
    <div className="home-page">
      <div className="home-cards-row home-cards-row--top">
        {CARDS.map((c) => (
          <button
            key={c.id}
            className="home-card"
            onClick={() => setPage(c.id)}
          >
            <span className="home-card-icon">{c.icon}</span>
            <span className="home-card-label">{c.label}</span>
          </button>
        ))}
      </div>

      {auth.currentUser && (
        <div className="home-user-bar">
          <span className="home-user-email">{auth.currentUser.email}</span>
          {auth.currentUser.isAdmin && (
            <span className="home-user-badge">admin</span>
          )}
          <button className="home-user-logout" onClick={logout}>Sign out</button>
        </div>
      )}
    </div>
  );
}
