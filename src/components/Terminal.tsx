import { useEffect, useRef } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebglAddon } from "@xterm/addon-webgl";
import { listen } from "@tauri-apps/api/event";
import * as api from "../services/api";
import "@xterm/xterm/css/xterm.css";

interface TerminalProps {
  sessionId: string;
  host: string;
  user: string;
  port?: number;
}

export default function Terminal({ sessionId, host, user, port }: TerminalProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const initialized = useRef(false);

  useEffect(() => {
    if (!containerRef.current || initialized.current) return;
    initialized.current = true;

    const term = new XTerm({
      theme: {
        background: "#0a0a0f",
        foreground: "#e4e4e7",
        cursor: "#6366f1",
        cursorAccent: "#0a0a0f",
        selectionBackground: "#6366f133",
        black: "#18181b",
        red: "#ef4444",
        green: "#22c55e",
        yellow: "#eab308",
        blue: "#3b82f6",
        magenta: "#a855f7",
        cyan: "#06b6d4",
        white: "#e4e4e7",
        brightBlack: "#52525b",
        brightRed: "#f87171",
        brightGreen: "#4ade80",
        brightYellow: "#facc15",
        brightBlue: "#60a5fa",
        brightMagenta: "#c084fc",
        brightCyan: "#22d3ee",
        brightWhite: "#fafafa",
      },
      fontFamily: '"SF Mono", "Fira Code", "Cascadia Code", Menlo, monospace',
      fontSize: 13,
      lineHeight: 1.2,
      cursorBlink: true,
      cursorStyle: "bar",
      scrollback: 10000,
      allowProposedApi: true,
    });

    const fitAddon = new FitAddon();
    term.loadAddon(fitAddon);
    term.open(containerRef.current);

    try {
      const webglAddon = new WebglAddon();
      term.loadAddon(webglAddon);
    } catch {
      // WebGL not available
    }

    fitAddon.fit();

    // User input → send as byte array to Rust PTY
    term.onData((data) => {
      const encoder = new TextEncoder();
      const bytes = Array.from(encoder.encode(data));
      api.sshWrite(sessionId, bytes).catch(console.error);
    });

    // Handle binary input (paste, etc.)
    term.onBinary((data) => {
      const bytes = [];
      for (let i = 0; i < data.length; i++) {
        bytes.push(data.charCodeAt(i));
      }
      api.sshWrite(sessionId, bytes).catch(console.error);
    });

    // Terminal resize → notify Rust PTY
    term.onResize(({ cols, rows }) => {
      api.sshResize(sessionId, cols, rows).catch(console.error);
    });

    // Receive PTY output as base64 string → decode → write to xterm
    const outputUnlisten = listen<string>(
      `terminal-output-${sessionId}`,
      (event) => {
        try {
          const binary = atob(event.payload);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
          }
          term.write(bytes);
        } catch (e) {
          console.error("Failed to decode terminal data:", e);
        }
      },
    );

    // Session ended
    const exitUnlisten = listen(`terminal-exit-${sessionId}`, () => {
      term.write("\r\n\x1b[90m[Session ended]\x1b[0m\r\n");
    });

    // Connect
    term.write(`\x1b[90mConnecting to ${user}@${host}...\x1b[0m\r\n`);
    api.sshOpen(sessionId, host, user, port).catch((err) => {
      term.write(`\x1b[31mConnection failed: ${err}\x1b[0m\r\n`);
    });

    // Auto-fit on container resize
    const resizeObserver = new ResizeObserver(() => {
      try {
        fitAddon.fit();
      } catch {
        // ignore
      }
    });
    resizeObserver.observe(containerRef.current);

    return () => {
      outputUnlisten.then((fn) => fn());
      exitUnlisten.then((fn) => fn());
      resizeObserver.disconnect();
      api.sshClose(sessionId).catch(() => {});
      term.dispose();
      initialized.current = false;
    };
  }, [sessionId, host, user]);

  return (
    <div ref={containerRef} className="w-full h-full bg-surface-0 p-1" />
  );
}
