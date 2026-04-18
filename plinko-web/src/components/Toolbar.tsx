import { useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { IconBack, IconPullMonday, IconPushMonday, IconSettings, IconSpinner } from "./icons";
import "./Toolbar.css";

type MondayOp = "pull" | "push" | null;

export function Toolbar() {
  const { plan, status, auth, page, setPage, previousPage, setPreviousPage, toolbarActions, toolbarRightActions, hasMondayIntegration, monday, sendRequest } = usePlanContext();

  const isHome = page === "home";
  // All hooks must come before any conditional return.
  const [activeOp, setActiveOp] = useState<MondayOp>(null);
  const [doneText, setDoneText] = useState<{ text: string; isError: boolean } | null>(null);
  const doneTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Track completion and errors — setState in effect is intentional here (timer-based auto-clear).
  useEffect(() => {
    if (monday.lastMessage) {
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActiveOp(null);
      setDoneText({ text: monday.lastMessage, isError: false });
      if (doneTimerRef.current) clearTimeout(doneTimerRef.current);
      doneTimerRef.current = setTimeout(() => setDoneText(null), 5000);
    }
    if (monday.lastError) {
      setActiveOp(null);
      setDoneText({ text: monday.lastError, isError: true });
      if (doneTimerRef.current) clearTimeout(doneTimerRef.current);
      doneTimerRef.current = setTimeout(() => setDoneText(null), 8000);
    }
  }, [monday.lastMessage, monday.lastError]);

  // Don't render toolbar on login/connecting screens — after all hooks.
  if (status === "connecting" || status === "handshaking" || status === "authenticating" || status === "disconnected" || status === "error" || auth.required) {
    return null;
  }

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

  const handlePull = () => {
    if (!plan || activeOp) return;
    setActiveOp("pull");
    setDoneText(null);
    sendRequest({ MondayPull: { plan_id: plan.id } }).catch(() => setActiveOp(null));
  };

  const handlePush = () => {
    if (!plan || activeOp) return;
    setActiveOp("push");
    setDoneText(null);
    sendRequest({ MondayPush: { plan_id: plan.id } }).catch(() => setActiveOp(null));
  };

  // Label shown in the floating status bar below the toolbar.
  const statusText = (() => {
    if (activeOp && monday.progress) {
      const { done, total, message } = monday.progress;
      const counter = total > 0 ? ` (${done}/${total})` : "";
      return `${message}${counter}`;
    }
    if (activeOp) return activeOp === "pull" ? "Fetching…" : "Preparing push…";
    if (doneText) return doneText.text;
    return null;
  })();
  const statusIsError = !activeOp && !!doneText?.isError;

  return (
    <div className="toolbar">
      <span className="toolbar-title">
        {plan?.name ?? "Plinko"}
      </span>
      <div className="toolbar-buttons-row">
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
        <div className="toolbar-right">
          {toolbarRightActions}
          {hasMondayIntegration && (
            <div className="monday-btn-group">
              <button
                className={`toolbar-btn${activeOp === "pull" ? " monday-op-btn--active" : ""}`}
                title="Pull from Monday"
                onClick={handlePull}
                disabled={!!activeOp}
              >
                {activeOp === "pull"
                  ? <IconSpinner size={16} color="#a0a8d0" />
                  : <IconPullMonday size={16} />}
              </button>
              <button
                className={`toolbar-btn${activeOp === "push" ? " monday-op-btn--active" : ""}`}
                title="Push to Monday"
                onClick={handlePush}
                disabled={!!activeOp}
              >
                {activeOp === "push"
                  ? <IconSpinner size={16} color="#a0a8d0" />
                  : <IconPushMonday size={16} />}
              </button>
            </div>
          )}
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
      {/* Status label floats below the toolbar, right-aligned */}
      {statusText && (
        <div
          className={`monday-status-bar${statusIsError ? " monday-status-bar--error" : ""}`}
          onClick={!activeOp ? () => setDoneText(null) : undefined}
          style={!activeOp ? { cursor: "pointer" } : undefined}
        >
          {statusText}
        </div>
      )}
    </div>
  );
}


