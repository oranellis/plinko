import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePlanContext } from "../context/PlanContext";
import { BugReportModal } from "./modals/BugReportModal";
import { IconBack, IconPullMonday, IconPushMonday, IconSettings, IconSpinner } from "./icons";
import "./Toolbar.css";

type MondayOp = "pull" | "push" | null;

interface PlanEntry { id: string; name: string }

function PlanSwitcherDropdown({
  anchor,
  currentPlanId,
  onSelect,
  onClose,
  sendRequest,
}: {
  anchor: HTMLElement | null;
  currentPlanId: string | null;
  onSelect: (id: string) => void;
  onClose: () => void;
  sendRequest: ReturnType<typeof usePlanContext>["sendRequest"];
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!anchor) return;
    const rect = anchor.getBoundingClientRect();
    const top = rect.bottom + 4;
    const dropW = 240;
    const left = Math.max(8, Math.min(rect.left + rect.width / 2 - dropW / 2, window.innerWidth - dropW - 8));
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setPos({ top, left });
  }, [anchor]);

  useEffect(() => {
    let cancelled = false;
    sendRequest("ListPlans").then((resp) => {
      if (cancelled) return;
      if (typeof resp === "object" && resp !== null && "PlanList" in resp) {
        const list = (resp as { PlanList: [string, string, string][] }).PlanList;
        setPlans(list.map(([id, name]) => ({ id, name })));
      }
    }).finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [sendRequest]);

  useEffect(() => {
    const handler = (e: PointerEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node) &&
          anchor && !anchor.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("pointerdown", handler);
    return () => document.removeEventListener("pointerdown", handler);
  }, [anchor, onClose]);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  if (!pos) return null;

  return createPortal(
    <div
      ref={containerRef}
      style={{
        position: "fixed",
        top: pos.top,
        left: pos.left,
        width: 240,
        zIndex: 9999,
        background: "#252526",
        border: "1px solid #3a3a3c",
        borderRadius: 6,
        boxShadow: "0 8px 24px rgba(0,0,0,0.6)",
        overflow: "hidden",
        maxHeight: 320,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <div style={{ overflowY: "auto", flex: 1 }}>
        {loading ? (
          <div style={{ padding: "10px 12px", fontSize: 13, color: "#888" }}>Loading plans…</div>
        ) : plans.length === 0 ? (
          <div style={{ padding: "10px 12px", fontSize: 13, color: "#888" }}>No plans available</div>
        ) : (
          plans.map((p) => {
            const isActive = p.id === currentPlanId;
            return (
              <button
                key={p.id}
                onMouseDown={(e) => { e.preventDefault(); onSelect(p.id); }}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  width: "100%",
                  textAlign: "left",
                  background: isActive ? "#1c3a5a" : "none",
                  border: "none",
                  padding: "8px 12px",
                  color: isActive ? "#7cb9f4" : "#d4d4d4",
                  fontSize: 13,
                  cursor: "pointer",
                  fontFamily: "inherit",
                }}
                onMouseEnter={(e) => { if (!isActive) e.currentTarget.style.background = "#37373d"; }}
                onMouseLeave={(e) => { e.currentTarget.style.background = isActive ? "#1c3a5a" : "none"; }}
              >
                <span style={{ width: 14, flexShrink: 0, color: "#4a90d9", fontSize: 11 }}>{isActive ? "✓" : ""}</span>
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{p.name}</span>
              </button>
            );
          })
        )}
      </div>
    </div>,
    document.body,
  );
}

export function Toolbar() {
  const { plan, status, auth, page, setPage, previousPage, setPreviousPage, toolbarActions, toolbarRightActions, hasMondayIntegration, monday, sendRequest } = usePlanContext();

  const isHome = page === "home";
  // All hooks must come before any conditional return.
  const [activeOp, setActiveOp] = useState<MondayOp>(null);
  const [doneText, setDoneText] = useState<{ text: string; isError: boolean } | null>(null);
  const [bugReportOpen, setBugReportOpen] = useState(false);
  const doneTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [switcherOpen, setSwitcherOpen] = useState(false);
  const [loadingPlanId, setLoadingPlanId] = useState<string | null>(null);
  const titleBtnRef = useRef<HTMLButtonElement>(null);

  // Track completion and errors — setState in effect is intentional here (timer-based auto-clear).
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

  const handleSelectPlan = async (planId: string) => {
    setSwitcherOpen(false);
    if (planId === plan?.id) return;
    setLoadingPlanId(planId);
    try {
      await sendRequest({ LoadPlan: { plan_id: planId } });
    } finally {
      setLoadingPlanId(null);
    }
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
      <button
        ref={titleBtnRef}
        className="toolbar-title"
        onClick={() => setSwitcherOpen((o) => !o)}
        title="Switch plan"
        aria-haspopup="listbox"
        aria-expanded={switcherOpen}
      >
        <span className="toolbar-title-text">
          {loadingPlanId ? "Loading…" : (plan?.name ?? "Plinko")}
        </span>
        <span className="toolbar-title-chevron">▾</span>
      </button>
      {switcherOpen && (
        <PlanSwitcherDropdown
          anchor={titleBtnRef.current}
          currentPlanId={plan?.id ?? null}
          onSelect={handleSelectPlan}
          onClose={() => setSwitcherOpen(false)}
          sendRequest={sendRequest}
        />
      )}
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
            title="Report a bug"
            onClick={() => setBugReportOpen(true)}
          >
            <svg viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" width="18" height="18">
              {/* body */}
              <ellipse cx="9" cy="10" rx="3.5" ry="4.5" />
              {/* head */}
              <circle cx="9" cy="4.5" r="1.8" />
              {/* antennae */}
              <path d="M8 3.2 Q6.5 1.5 5 1" />
              <path d="M10 3.2 Q11.5 1.5 13 1" />
              {/* legs */}
              <path d="M5.5 8 L2.5 7" />
              <path d="M5.5 10 L2 10" />
              <path d="M5.5 12 L2.5 13" />
              <path d="M12.5 8 L15.5 7" />
              <path d="M12.5 10 L16 10" />
              <path d="M12.5 12 L15.5 13" />
            </svg>
          </button>
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
      {bugReportOpen && (
        <BugReportModal sendRequest={sendRequest} onClose={() => setBugReportOpen(false)} />
      )}
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


