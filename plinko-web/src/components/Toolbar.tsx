import { usePlanContext } from "../context/PlanContext";
import { IconBack } from "./icons";
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
        <span
          className={`toolbar-status toolbar-status--${status}`}
          title={status}
        />
      </div>
    </div>
  );
}
