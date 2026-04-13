import React, { useState } from "react";
import { usePlanContext } from "../context/PlanContext";
import plinkoLogo from "../assets/plinko_logo.svg";
import "./LoginPage.css";

export function LoginPage() {
  const { auth, login, status } = usePlanContext();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");

  const isLoading = status === "authenticating" && !auth.required;

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (email && password) {
      login(email, password);
    }
  }

  return (
    <div className="login-page">
      <div className="login-card">
        <div className="login-logo">
          <img src={plinkoLogo} alt="Plinko logo" />
        </div>

        <h1 className="login-title">Plinko</h1>
        <p className="login-subtitle">Sign in to continue</p>

        <form className="login-form" onSubmit={handleSubmit}>
          <div className="login-field">
            <label htmlFor="login-email">Email</label>
            <input
              id="login-email"
              type="email"
              autoComplete="email"
              autoFocus
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@example.com"
              required
              disabled={isLoading}
            />
          </div>

          <div className="login-field">
            <label htmlFor="login-password">Password</label>
            <input
              id="login-password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••"
              required
              disabled={isLoading}
            />
          </div>

          {auth.loginError && (
            <p className="login-error">{auth.loginError}</p>
          )}

          <button
            type="submit"
            className="login-btn"
            disabled={isLoading || !email || !password}
          >
            {isLoading ? "Signing in…" : "Sign in"}
          </button>
        </form>
      </div>
    </div>
  );
}
