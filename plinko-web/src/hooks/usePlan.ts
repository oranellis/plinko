import { useCallback, useEffect, useRef, useState } from "react";
import type { ClientMessage, Plan, PlanRequest, PlanResponse, ServerMessage } from "../protocol";
import { PROTOCOL_VERSION } from "../protocol";

export type ConnectionStatus = "connecting" | "handshaking" | "connected" | "disconnected" | "error";

export interface MondayState {
  progress: { done: number; total: number; message: string } | null;
  lastMessage: string | null;
  lastError: string | null;
}

export interface UsePlanResult {
  plan: Plan | null;
  status: ConnectionStatus;
  monday: MondayState;
  hasMondayIntegration: boolean;
  sendRequest: (request: PlanRequest) => Promise<PlanResponse>;
}

const WS_PORT = 7892; // TCP port + 1

export function usePlan(): UsePlanResult {
  const [plan, setPlan] = useState<Plan | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [hasMondayIntegration, setHasMondayIntegration] = useState(false);
  const [monday, setMonday] = useState<MondayState>({
    progress: null,
    lastMessage: null,
    lastError: null,
  });

  const wsRef = useRef<WebSocket | null>(null);
  const nextIdRef = useRef<number>(1);
  // Map from request id → { resolve, reject }
  const pendingRef = useRef<
    Map<number, { resolve: (r: PlanResponse) => void; reject: (e: Error) => void }>
  >(new Map());

  const sendRequest = useCallback((request: PlanRequest): Promise<PlanResponse> => {
    return new Promise((resolve, reject) => {
      const ws = wsRef.current;
      if (!ws || ws.readyState !== WebSocket.OPEN) {
        reject(new Error("Not connected"));
        return;
      }
      const id = nextIdRef.current++;
      pendingRef.current.set(id, { resolve, reject });
      const msg: ClientMessage = { type: "Request", id, request };
      ws.send(JSON.stringify(msg));
    });
  }, []);

  useEffect(() => {
    let ws: WebSocket;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
    let alive = true;

    function connect() {
      setStatus("connecting");
      ws = new WebSocket(`ws://${window.location.hostname}:${WS_PORT}`);
      wsRef.current = ws;

      ws.onopen = () => {
        setStatus("handshaking");
        const hello: ClientMessage = { type: "Hello", version: PROTOCOL_VERSION };
        ws.send(JSON.stringify(hello));
      };

      ws.onmessage = (evt) => {
        let msg: ServerMessage;
        try {
          msg = JSON.parse(evt.data as string) as ServerMessage;
        } catch {
          console.error("[ws] failed to parse message", evt.data);
          return;
        }

        switch (msg.type) {
          case "Hello":
            // Server greeting — nothing to do, we've already sent our Hello.
            break;

          case "VersionError":
            console.error(`[ws] version mismatch: server expects ${msg.expected}, we sent ${msg.got}`);
            setStatus("error");
            ws.close();
            break;

          case "PlanState":
            setPlan(msg.plan);
            setHasMondayIntegration(msg.has_monday_integration);
            if (status !== "connected") setStatus("connected");
            break;

          case "Response": {
            const pending = pendingRef.current.get(msg.id);
            if (pending) {
              pendingRef.current.delete(msg.id);
              pending.resolve(msg.response);
            }
            break;
          }

          case "MondayProgress":
            setMonday((prev) => ({
              ...prev,
              progress: { done: msg.done, total: msg.total, message: msg.message },
              lastError: null,
            }));
            break;

          case "MondayDone":
            setMonday((prev) => ({
              ...prev,
              progress: null,
              lastMessage: msg.message,
              lastError: null,
            }));
            break;

          case "MondayError":
            setMonday((prev) => ({
              ...prev,
              progress: null,
              lastError: msg.message,
            }));
            break;
        }

        // Mark connected after first PlanState
        if (msg.type === "PlanState") {
          setStatus("connected");
        }
      };

      ws.onerror = (e) => {
        console.error("[ws] error", e);
      };

      ws.onclose = () => {
        // Only clear wsRef if this is still the active WebSocket — in React
        // StrictMode the effect runs twice; the first WS's onclose fires after
        // the second WS has already been stored in wsRef, so we must not null it.
        if (wsRef.current === ws) wsRef.current = null;
        // Reject all pending requests
        for (const { reject } of pendingRef.current.values()) {
          reject(new Error("WebSocket closed"));
        }
        pendingRef.current.clear();

        if (alive) {
          setStatus("disconnected");
          reconnectTimer = setTimeout(connect, 2000);
        }
      };
    }

    connect();

    return () => {
      alive = false;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      const currentWs = wsRef.current;
      wsRef.current = null;
      currentWs?.close();
      // Reset status so that when the effect re-runs (React StrictMode) and WS
      // reconnects, status transitions "connecting" → "connected" and any
      // dependent effects (e.g. SettingsPage plan list fetch) re-fire correctly.
      setStatus("connecting");
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return { plan, status, monday, hasMondayIntegration, sendRequest };
}
