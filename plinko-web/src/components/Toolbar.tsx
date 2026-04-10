import React, { useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { IconBack, IconPullMonday, IconPushMonday, IconSettings } from "./icons";
import "./Toolbar.css";

type MondayOp = "pull" | "push" | null;

export function Toolbar() {
  const { plan, status, page, setPage, previousPage, setPreviousPage, toolbarActions, toolbarRightActions, hasMondayIntegration, monday, sendRequest } = usePlanContext();

  const isHome = page === "home";
  const [activeOp, setActiveOp] = useState<MondayOp>(null);
  const [doneText, setDoneText] = useState<{ text: string; isError: boolean } | null>(null);
  const doneTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Track completion and errors, show brief done text then clear.
  useEffect(() => {
    if (monday.lastMessage) {
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

  // Build the label shown inside the active button.
  const progressLabel = (() => {
    if (!monday.progress) return null;
    const { done, total, message } = monday.progress;
    const counter = total > 0 ? ` (${done}/${total})` : "";
    return `${message}${counter}`;
  })();

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
        {hasMondayIntegration && (
          <div className="monday-btn-group">
            {/* Pull button — expands when active */}
            <button
              className={`toolbar-btn monday-op-btn${activeOp === "pull" ? " monday-op-btn--active" : ""}`}
              title="Pull from Monday"
              onClick={handlePull}
              disabled={!!activeOp}
            >
              <IconPullMonday size={16} />
              {activeOp === "pull" && (
                <span className="monday-op-label">
                  {progressLabel ?? "Fetching…"}
                </span>
              )}
            </button>
            {/* Push button — expands when active */}
            <button
              className={`toolbar-btn monday-op-btn${activeOp === "push" ? " monday-op-btn--active" : ""}`}
              title="Push to Monday"
              onClick={handlePush}
              disabled={!!activeOp}
            >
              <IconPushMonday size={16} />
              {activeOp === "push" && (
                <span className="monday-op-label">
                  {progressLabel ?? "Preparing…"}
                </span>
              )}
            </button>
            {/* Completion / error chip */}
            {doneText && !activeOp && (
              <span
                className={`monday-done-chip${doneText.isError ? " monday-done-chip--error" : ""}`}
                onClick={() => setDoneText(null)}
                title="Click to dismiss"
              >
                {doneText.text}
              </span>
            )}
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
  );
}

