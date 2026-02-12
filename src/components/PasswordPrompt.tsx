import { useState } from "react";
import { Lock, Loader2 } from "lucide-react";
import * as api from "../services/api";

interface PasswordPromptProps {
  onUnlocked: () => void;
}

export default function PasswordPrompt({ onUnlocked }: PasswordPromptProps) {
  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleUnlock = async () => {
    if (!password) return;
    setLoading(true);
    setError(null);
    try {
      await api.unlockConfig(password);
      onUnlocked();
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-center h-screen bg-surface-0">
      <div className="w-80 p-6 bg-surface-2 rounded-xl border border-surface-3">
        <div className="flex items-center justify-center mb-4">
          <Lock className="w-8 h-8 text-accent" />
        </div>
        <h2 className="text-center text-lg font-semibold text-gray-900 dark:text-white mb-1">RemoteLab</h2>
        <p className="text-center text-sm text-gray-500 dark:text-gray-400 mb-4">Enter password to unlock</p>
        <input
          type="password"
          placeholder="Password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleUnlock()}
          className="w-full px-3 py-2 bg-surface-0 border border-surface-3 rounded text-sm text-gray-900 dark:text-white placeholder-gray-400 focus:outline-none focus:border-accent mb-3"
          autoFocus
        />
        {error && <p className="text-xs text-red-400 mb-2">{error}</p>}
        <button
          onClick={handleUnlock}
          disabled={loading || !password}
          className="w-full px-3 py-2 bg-accent hover:bg-accent-hover text-white rounded text-sm transition-colors disabled:opacity-50"
        >
          {loading ? <Loader2 className="w-4 h-4 animate-spin mx-auto" /> : "Unlock"}
        </button>
      </div>
    </div>
  );
}
