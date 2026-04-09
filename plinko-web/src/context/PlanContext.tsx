import React, { createContext, useContext } from "react";
import { Plan, PlanRequest, PlanResponse } from "../protocol";
import { ConnectionStatus, MondayState, usePlan } from "../hooks/usePlan";

export type PageId =
  | "home"
  | "overview"
  | "allocation"
  | "calendar"
  | "daily"
  | "settings";

interface PlanContextValue {
  plan: Plan | null;
  status: ConnectionStatus;
  monday: MondayState;
  sendRequest: (req: PlanRequest) => Promise<PlanResponse>;
  page: PageId;
  setPage: (p: PageId) => void;
}

const PlanContext = createContext<PlanContextValue | null>(null);

export function PlanProvider({ children }: { children: React.ReactNode }) {
  const planData = usePlan();
  const [page, setPage] = React.useState<PageId>("home");

  return (
    <PlanContext.Provider value={{ ...planData, page, setPage }}>
      {children}
    </PlanContext.Provider>
  );
}

export function usePlanContext(): PlanContextValue {
  const ctx = useContext(PlanContext);
  if (!ctx) throw new Error("usePlanContext must be used within PlanProvider");
  return ctx;
}
