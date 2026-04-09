import { PageId, usePlanContext } from "../context/PlanContext";
import "./HomePage.css";

interface NavCard {
  id: PageId;
  label: string;
  icon: string;
}

const CARDS: NavCard[] = [
  { id: "daily", label: "Daily", icon: "📅" },
  { id: "overview", label: "Overview", icon: "📊" },
  { id: "settings", label: "Settings", icon: "⚙️" },
  { id: "allocation", label: "Allocation", icon: "👥" },
  { id: "calendar", label: "Calendar", icon: "🗓️" },
];

export function HomePage() {
  const { setPage } = usePlanContext();

  return (
    <div className="home-page">
      <div className="home-cards-row home-cards-row--top">
        {CARDS.slice(0, 3).map((c) => (
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
      <div className="home-cards-row home-cards-row--bottom">
        {CARDS.slice(3).map((c) => (
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
    </div>
  );
}
