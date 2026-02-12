import { useState, useEffect } from "react";
import { TerminalSquare, ChevronUp, ChevronDown } from "lucide-react";
import Dashboard from "./components/Dashboard";
import Terminal from "./components/Terminal";
import RemoteDesktop from "./components/RemoteDesktop";
import FileManager from "./components/FileManager";
import Settings from "./components/Settings";
import PasswordPrompt from "./components/PasswordPrompt";
import * as api from "./services/api";

type View = "dashboard" | "settings" | "remote-desktop" | "file-manager";

interface TerminalSession {
  id: string;
  deviceName: string;
  host: string;
  user: string;
  port?: number;
}

interface DesktopSession {
  deviceName: string;
  host: string;
  user: string;
  port?: number;
}

interface FileSession {
  deviceName: string;
  host: string;
  user: string;
  port?: number;
}

function App() {
  const [locked, setLocked] = useState<boolean | null>(null);
  const [view, setView] = useState<View>("dashboard");
  const [terminalSessions, setTerminalSessions] = useState<TerminalSession[]>([]);
  const [activeTerminal, setActiveTerminal] = useState<string | null>(null);
  const [terminalExpanded, setTerminalExpanded] = useState(false);
  const [desktopSession, setDesktopSession] = useState<DesktopSession | null>(null);
  const [fileSession, setFileSession] = useState<FileSession | null>(null);

  useEffect(() => {
    api.configIsEncrypted().then((encrypted) => setLocked(encrypted)).catch(() => setLocked(false));
  }, []);

  const openTerminal = (deviceName: string, host: string, user: string, port?: number) => {
    // If session to same host exists, just activate it
    const existing = terminalSessions.find(
      (s) => s.host === host && s.user === user && s.port === port,
    );
    if (existing) {
      setActiveTerminal(existing.id);
      setTerminalExpanded(true);
      return;
    }
    const id = `${host.replace(/\./g, "_")}-${Date.now()}`;
    setTerminalSessions((prev) => [...prev, { id, deviceName, host, user, port }]);
    setActiveTerminal(id);
    setTerminalExpanded(true);
  };

  const closeTerminal = (id: string) => {
    setTerminalSessions((prev) => {
      const remaining = prev.filter((s) => s.id !== id);
      if (remaining.length === 0) {
        setTerminalExpanded(false);
        setActiveTerminal(null);
      } else if (activeTerminal === id) {
        setActiveTerminal(remaining[0].id);
      }
      return remaining;
    });
  };

  const openDesktop = (deviceName: string, host: string, user: string, port?: number) => {
    setDesktopSession({ deviceName, host, user, port });
    setView("remote-desktop");
  };

  const closeDesktop = () => {
    setDesktopSession(null);
    setView("dashboard");
  };

  const openFiles = (deviceName: string, host: string, user: string, port?: number) => {
    setFileSession({ deviceName, host, user, port });
    setView("file-manager");
  };

  const closeFiles = () => {
    setFileSession(null);
    setView("dashboard");
  };

  const hasTerminals = terminalSessions.length > 0;

  // Show loading while checking encryption status
  if (locked === null) {
    return <div className="flex items-center justify-center h-screen bg-surface-0" />;
  }

  // Show password prompt if config is encrypted
  if (locked) {
    return <PasswordPrompt onUnlocked={() => setLocked(false)} />;
  }

  return (
    <div className="flex flex-col h-screen bg-surface-0">
      {/* Main content — shrinks when terminal is expanded */}
      <div
        className="overflow-hidden"
        style={{ flex: terminalExpanded && hasTerminals ? "1 1 50%" : "1 1 100%" }}
      >
        {view === "dashboard" && (
          <Dashboard
            onOpenTerminal={openTerminal}
            onOpenDesktop={openDesktop}
            onOpenFiles={openFiles}
            onNavigate={(v) => setView(v as View)}
          />
        )}
        {view === "settings" && (
          <Settings onNavigate={(v) => setView(v as View)} />
        )}
        {view === "remote-desktop" && desktopSession && (
          <RemoteDesktop
            deviceName={desktopSession.deviceName}
            host={desktopSession.host}
            user={desktopSession.user}
            port={desktopSession.port}
            onClose={closeDesktop}
          />
        )}
        {view === "file-manager" && fileSession && (
          <FileManager
            deviceName={fileSession.deviceName}
            host={fileSession.host}
            user={fileSession.user}
            port={fileSession.port}
            onClose={closeFiles}
          />
        )}
      </div>

      {/* Terminal panel — always shows tab bar when sessions exist */}
      {hasTerminals && (
        <div
          className="border-t border-surface-3 flex flex-col shrink-0"
          style={{ height: terminalExpanded ? "50%" : "auto" }}
        >
          {/* Tab bar — always visible */}
          <div className="flex items-center bg-surface-1 px-2 h-9 gap-1 shrink-0">
            <button
              onClick={() => setTerminalExpanded(!terminalExpanded)}
              className="flex items-center gap-1.5 text-gray-500 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white text-xs px-1.5 py-1 rounded hover:bg-surface-3 transition-colors mr-1"
              title={terminalExpanded ? "Collapse terminal" : "Expand terminal"}
            >
              <TerminalSquare className="w-3.5 h-3.5" />
              {terminalExpanded ? (
                <ChevronDown className="w-3 h-3" />
              ) : (
                <ChevronUp className="w-3 h-3" />
              )}
            </button>

            {terminalSessions.map((session) => (
              <button
                key={session.id}
                onClick={() => {
                  setActiveTerminal(session.id);
                  setTerminalExpanded(true);
                }}
                className={`flex items-center gap-1.5 px-3 py-1 rounded text-xs transition-colors ${
                  activeTerminal === session.id && terminalExpanded
                    ? "bg-surface-3 text-gray-900 dark:text-white"
                    : "text-gray-500 dark:text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 hover:bg-surface-3/50"
                }`}
              >
                <span className="w-1.5 h-1.5 rounded-full bg-green-400 shrink-0" />
                <span className="truncate max-w-[120px]">{session.deviceName}</span>
                <span
                  onClick={(e) => {
                    e.stopPropagation();
                    closeTerminal(session.id);
                  }}
                  className="ml-1 hover:text-red-400 cursor-pointer text-[10px]"
                >
                  ×
                </span>
              </button>
            ))}
          </div>

          {/* Terminal content — only when expanded */}
          {terminalExpanded && (
            <div className="flex-1 overflow-hidden">
              {terminalSessions.map((session) => (
                <div
                  key={session.id}
                  className={`h-full ${activeTerminal === session.id ? "" : "hidden"}`}
                >
                  <Terminal
                    sessionId={session.id}
                    host={session.host}
                    user={session.user}
                    port={session.port}
                  />
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default App;
