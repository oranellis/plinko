import React, { useEffect, useRef, useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import { IconBack, IconPullMonday, IconPushMonday, IconSettings } from "./icons";
import "./Toolbar.css";

export function Toolbar() {
  const { plan, status, page, setPage, previousPage, setPreviousPage, toolbarActions, toolbarRightActions, hasMondayIntegration, monday, sendRequest } = usePlanContext();

  const isHome = page === "home";
  const [toast, setToast] = useState<{ text: string; isError: boolean } | null>(null);
  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [mondayBusy, setMondayBusy] = useState(false);

  // Show toast when monday state changes.
  useEffect(() => {
    if (monday.lastMessage) {
      setToast({ text: monday.lastMessage, isError: false });
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
      toastTimerRef.current = setTimeout(() => setToast(null), 4000);
      setMondayBusy(false);
    }
    if (monday.lastError) {
      setToast({ text: monday.lastError, isError: true });
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
      toastTimerRef.current = setTimeout(() => setToast(null), 7000);
      setMondayBusy(false);
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
    if (!plan || mondayBusy) return;
    setMondayBusy(true);
    setToast({ text: "Pulling from Monday…", isError: false });
    sendRequest({ MondayPull: { plan_id: plan.id } }).catch(() => setMondayBusy(false));
  };

  const handlePush = () => {
    if (!plan || mondayBusy) return;
    setMondayBusy(true);
    setToast({ text: "Pushing to Monday…", isError: false });
    sendRequest({ MondayPush: { plan_id: plan.id } }).catch(() => setMondayBusy(false));
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
        {hasMondayIntegration && (
          <>
            <button
              className="toolbar-btn"
              title="Pull from Monday"
              onClick={handlePull}
              disabled={mondayBusy}
              style={mondayBusy ? { opacity: 0.5 } : undefined}
            >
              <IconPullMonday size={18} />
            </button>
            <button
              className="toolbar-btn"
              title="Push to Monday"
              onClick={handlePush}
              disabled={mondayBusy}
              style={mondayBusy ? { opacity: 0.5 } : undefined}
            >
              <IconPushMonday size={18} />
            </button>
          </>
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
      {toast && (
        <div
          className={`toolbar-toast${toast.isError ? " toolbar-toast--error" : ""}`}
          onClick={() => setToast(null)}
        >
          {toast.text}
        </div>
      )}
    </div>
  );
}
