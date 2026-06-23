import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import { AgentmuxTerminalApp } from "./agentmux/AgentmuxTerminalApp";
import "./fonts.css";
import "./styles.css";

// The implemented design is the primary product UI. The original backend-wired
// developer console remains reachable at "#console" (and is what the Playwright
// UI tests target).
function Root() {
  const [hash, setHash] = useState(() => window.location.hash);

  useEffect(() => {
    const onHashChange = () => setHash(window.location.hash);
    window.addEventListener("hashchange", onHashChange);
    return () => window.removeEventListener("hashchange", onHashChange);
  }, []);

  return hash === "#console" ? <App /> : <AgentmuxTerminalApp />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>
);
