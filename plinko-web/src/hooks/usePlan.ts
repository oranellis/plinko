import { useCallback, useEffect, useRef, useState } from "react";
import type { AuthUser, ClientMessage, Plan, PlanRequest, PlanResponse, ServerMessage } from "../protocol";
import { PROTOCOL_VERSION } from "../protocol";

export type ConnectionStatus = "connecting" | "handshaking" | "authenticating" | "connected" | "disconnected" | "error";

export interface MondayState {
  progress: { done: number; total: number; message: string } | null;
  lastMessage: string | null;
  lastError: string | null;
}

export interface AuthState {
  required: boolean;           // server has sent AuthRequired and we're not yet authenticated
  currentUser: { userId: string; email: string; isAdmin: boolean } | null;
  sessionToken: string | null;
  loginError: string | null;
}

export interface UsePlanResult {
  plan: Plan | null;
  status: ConnectionStatus;
  monday: MondayState;
  auth: AuthState;
  hasMondayIntegration: boolean;
  sendRequest: (request: PlanRequest) => Promise<PlanResponse>;
  login: (email: string, password: string) => void;
  logout: () => void;
}

const WS_PORT = 7892; // TCP port + 1
const SESSION_TOKEN_KEY = "plinko_session_token";

export function usePlan(): UsePlanResult {
  const [plan, setPlan] = useState<Plan | null>(null);
  const [status, setStatus] = useState<ConnectionStatus>("connecting");
  const [hasMondayIntegration, setHasMondayIntegration] = useState(false);
  const [monday, setMonday] = useState<MondayState>({
    progress: null,
    lastMessage: null,
    lastError: null,
  });
  const [auth, setAuth] = useState<AuthState>({
    required: false,
    currentUser: null,
    sessionToken: null,
    loginError: null,
  });

  const wsRef = useRef<WebSocket | null>(null);
  const nextIdRef = useRef<number>(1);
  const pendingRef = useRef<
    Map<number, { resolve: (r: PlanResponse) => void; reject: (e: Error) => void }>
  >(new Map());

  // Send a raw ClientMessage immediately (fire-and-forget).
  const sendRaw = useCallback((msg: ClientMessage) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(msg));
    }
  }, []);

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

  const login = useCallback((email: string, password: string) => {
    setAuth((prev) => ({ ...prev, loginError: null }));
    sendRaw({ type: "Login", email, password });
  }, [sendRaw]);

  const logout = useCallback(() => {
    sendRaw({ type: "Logout" });
    localStorage.removeItem(SESSION_TOKEN_KEY);
    setAuth({ required: true, currentUser: null, sessionToken: null, loginError: null });
    setPlan(null);
    setStatus("authenticating");
  }, [sendRaw]);

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
            break;

          case "VersionError":
            console.error(`[ws] version mismatch: server expects ${msg.expected}, we sent ${msg.got}`);
            setStatus("error");
            ws.close();
            break;

          case "AuthRequired":
            setStatus("authenticating");
            // Try to resume with a stored token first.
            const stored = localStorage.getItem(SESSION_TOKEN_KEY);
            if (stored) {
              ws.send(JSON.stringify({ type: "Authenticate", session_token: stored } as ClientMessage));
            } else {
              setAuth((prev) => ({ ...prev, required: true }));
            }
            break;

          case "LoginSuccess":
            localStorage.setItem(SESSION_TOKEN_KEY, msg.session_token);
            setAuth({
              required: false,
              currentUser: { userId: msg.user_id, email: msg.email, isAdmin: msg.is_admin },
              sessionToken: msg.session_token,
              loginError: null,
            });
            // Status remains "authenticating" until PlanState arrives.
            break;

          case "LoginFailed":
            // Clear stored token if it was rejected.
            localStorage.removeItem(SESSION_TOKEN_KEY);
            setAuth((prev) => ({
              ...prev,
              required: true,
              currentUser: null,
              sessionToken: null,
              loginError: msg.message,
            }));
            break;

          case "PlanState":
            setPlan(msg.plan);
            setHasMondayIntegration(msg.has_monday_integration);
            setStatus("connected");
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
      };

      ws.onerror = (e) => {
        console.error("[ws] error", e);
      };

      ws.onclose = () => {
        if (wsRef.current === ws) wsRef.current = null;
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
      setStatus("connecting");
    };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  return { plan, status, monday, auth, hasMondayIntegration, sendRequest, login, logout };
}
