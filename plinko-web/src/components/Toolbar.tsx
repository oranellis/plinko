import { usePlanContext } from "../context/PlanContext";
import { IconBack } from "./icons";
import "./Toolbar.css";

export function Toolbar() {
  const { plan, status, page, setPage } = usePlanContext();

  const isHome = page === "home";

  return (
    <div className="toolbar">
      {!isHome && (
        <button
          className="toolbar-back"
          onClick={() => setPage("home")}
          title="Home"
        >
          <IconBack size={15} />
        </button>
      )}
      <span className="toolbar-title">
        {plan?.name ?? "Plinko"}
      </span>
      <span className="toolbar-spacer" />
      <span
        className={`toolbar-status toolbar-status--${status}`}
        title={status}
      />
    </div>
  );
}
