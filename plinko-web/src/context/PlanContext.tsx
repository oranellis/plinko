import React, { createContext, useContext } from "react";
import type { Plan, PlanRequest, PlanResponse } from "../protocol";
import { type AuthState, type ConnectionStatus, type MondayState, usePlan } from "../hooks/usePlan";

export type PageId =
  | "home"
  | "overview"
  | "allocation"
  | "resources"
  | "daily"
  | "settings"
  | "admin";

interface PlanContextValue {
  plan: Plan | null;
  status: ConnectionStatus;
  monday: MondayState;
  auth: AuthState;
  hasMondayIntegration: boolean;
  remoteUpdate: boolean;
  sendRequest: (req: PlanRequest) => Promise<PlanResponse>;
  login: (email: string, password: string) => void;
  logout: () => void;
  reconnect: () => void;
  setActiveOrg: (orgId: string) => Promise<void>;
  page: PageId;
  setPage: (p: PageId) => void;
  previousPage: PageId | null;
  setPreviousPage: (p: PageId | null) => void;
  toolbarActions: React.ReactNode;
  setToolbarActions: (actions: React.ReactNode) => void;
  toolbarRightActions: React.ReactNode;
  setToolbarRightActions: (actions: React.ReactNode) => void;
}

const PlanContext = createContext<PlanContextValue | null>(null);

export function PlanProvider({ children }: { children: React.ReactNode }) {
  const planData = usePlan();
  const initialPage: PageId = window.location.pathname === "/admin" ? "admin" : "home";
  const [page, setPageState] = React.useState<PageId>(initialPage);
  const [previousPage, setPreviousPage] = React.useState<PageId | null>(null);
  const [toolbarActions, setToolbarActions] = React.useState<React.ReactNode>(null);
  const [toolbarRightActions, setToolbarRightActions] = React.useState<React.ReactNode>(null);

  const setPage = React.useCallback((p: PageId) => {
    setPageState(p);
    if (p === "admin") {
      window.history.pushState({}, "", "/admin");
    } else if (window.location.pathname !== "/") {
      window.history.pushState({}, "", "/");
    }
  }, []);

  React.useEffect(() => {
    const handler = () => {
      const path = window.location.pathname;
      setPageState(path === "/admin" ? "admin" : "home");
    };
    window.addEventListener("popstate", handler);
    return () => window.removeEventListener("popstate", handler);
  }, []);

  return (
    <PlanContext.Provider value={{ ...planData, page, setPage, previousPage, setPreviousPage, toolbarActions, setToolbarActions, toolbarRightActions, setToolbarRightActions }}>
      {children}
    </PlanContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function usePlanContext(): PlanContextValue {
  const ctx = useContext(PlanContext);
  if (!ctx) throw new Error("usePlanContext must be used within PlanProvider");
  return ctx;
}
