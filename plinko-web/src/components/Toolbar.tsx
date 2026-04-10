import { usePlanContext } from "../context/PlanContext";
import { IconBack, IconSettings } from "./icons";
import "./Toolbar.css";

export function Toolbar() {
  const { plan, status, page, setPage, previousPage, setPreviousPage, toolbarActions, toolbarRightActions } = usePlanContext();

  const isHome = page === "home";

  const handleBack = () => {
    if (page === "settings" && previousPage) {
      setPage(previousPage);
      setPreviousPage(null);
    } else {
      setPage("home");
    }
  };

  const handleSettings = () => {
    if (page === "settings") {
      setPage(previousPage ?? "home");
      setPreviousPage(null);
    } else {
      setPreviousPage(page);
      setPage("settings");
    }
  };

  return (
    <div className="toolbar">
      <div className="toolbar-left">
        {!isHome && (
          <button
            className="toolbar-back"
            onClick={handleBack}
            title="Back"
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
          onClick={handleSettings}
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
