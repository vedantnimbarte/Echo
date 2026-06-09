import { useEffect, useState } from "react";
import clsx from "clsx";
import { BookOpen, Clock, Settings, Puzzle } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useEchoEvents } from "./hooks/useEchoEvents";
import { DictionaryPanel } from "./components/dictionary/DictionaryPanel";
import { HistoryPanel } from "./components/history/HistoryPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { PluginsPanel } from "./components/plugins/PluginsPanel";

type Tab = "settings" | "dictionary" | "history" | "plugins";

const TABS: { id: Tab; label: string; Icon: React.ElementType }[] = [
  { id: "settings", label: "Settings", Icon: Settings },
  { id: "dictionary", label: "Dictionary", Icon: BookOpen },
  { id: "history", label: "History", Icon: Clock },
  { id: "plugins", label: "Plugins", Icon: Puzzle },
];

export default function App() {
  // The settings window observes state only — the pill owns the hotkey toggle.
  useEchoEvents();
  const [tab, setTab] = useState<Tab>("settings");

  // Keep this window alive when closed so the pill's gear can reopen it.
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    void win
      .onCloseRequested((e) => {
        e.preventDefault();
        void win.hide();
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-[#0a0b11] text-[var(--ink)] select-none">
      {/* Ambient aurora wash */}
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-48 opacity-50"
        style={{
          background:
            "radial-gradient(60% 100% at 30% 0%, rgba(52,231,228,0.12), transparent 70%), radial-gradient(60% 100% at 80% 0%, rgba(160,107,255,0.12), transparent 70%)",
        }}
      />

      {/* Sidebar + content */}
      <div className="relative flex min-h-0 flex-1">
        <nav className="flex w-[168px] flex-shrink-0 flex-col gap-0.5 border-r border-white/6 p-3">
          <div className="mb-3 flex items-center gap-2 px-2">
            <span
              className="h-2 w-2 rounded-full"
              style={{ background: "var(--aurora-1)", boxShadow: "0 0 8px var(--aurora-1)" }}
            />
            <span className="text-[13px] font-semibold tracking-tight">Echo</span>
          </div>
          {TABS.map(({ id, label, Icon }) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={clsx(
                "flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] tracking-tight transition-colors",
                tab === id
                  ? "bg-white/8 text-[var(--ink)]"
                  : "text-[var(--ink-muted)] hover:bg-white/4 hover:text-[var(--ink)]"
              )}
            >
              <Icon className="h-[15px] w-[15px]" />
              {label}
            </button>
          ))}
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto">
          {tab === "settings" && <SettingsPanel />}
          {tab === "dictionary" && <DictionaryPanel />}
          {tab === "history" && <HistoryPanel />}
          {tab === "plugins" && <PluginsPanel />}
        </main>
      </div>
    </div>
  );
}
