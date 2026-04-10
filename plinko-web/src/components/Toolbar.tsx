import { usePlanContext } from "../context/PlanContext";
import { IconBack, IconSettings } from "./icons";
import "./Toolbar.css";

export function Toolbar() {
  const { plan, status, page, setPage, toolbarActions, toolbarRightActions } = usePlanContext();

  const isHome = page === "home";

  return (
    <div className="toolbar">
      <div className="toolbar-left">
        {!isHome && (
          <button
            className="toolbar-back"
            onClick={() => setPage("home")}
            title="Home"
          >
            <IconBack size={15} />
          </button>
        )}
        {toolbarActions}
      </div>
      <span className="toolbar-title">
        {plan?.name ?? "Plinko"}
      </span>
      <div className="toolbar-right">
        {toolbarRightActions}
        <button
          className="toolbar-btn"
          title="Settings"
          onClick={() => setPage(page === "settings" ? "home" : "settings")}
          style={page === "settings" ? { color: "#a78bfa" } : undefined}
        >
          <IconSettings size={18} />
        </button>
        <span
          className={`toolbar-status toolbar-status--${status}`}
          title={status}
        />
      </div>
    </div>
  );
}
