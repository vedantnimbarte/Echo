import { useEffect, useState } from "react";
import clsx from "clsx";
import { BookOpen, Clock, Settings, Puzzle } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useEchoEvents } from "./hooks/useEchoEvents";
import { commands } from "./ipc/commands";
import { checkForUpdate } from "./update";
import { DictionaryPanel } from "./components/dictionary/DictionaryPanel";
import { HistoryPanel } from "./components/history/HistoryPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { PluginsPanel } from "./components/plugins/PluginsPanel";
import { Onboarding } from "./components/onboarding/Onboarding";

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

  // First run shows the onboarding wizard until it's marked complete.
  const { data: onboardingDone, isLoading: onboardingLoading } = useQuery({
    queryKey: ["setting", "onboarding_complete"],
    queryFn: () => commands.getSetting("onboarding_complete"),
  });

  // Check for a new release once on startup (silent if the updater isn't set up).
  useEffect(() => {
    void checkForUpdate();
  }, []);

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

  if (!onboardingLoading && onboardingDone !== "true") {
    // onDone is a no-op: finishing invalidates the query above, which refetches
    // "true" and re-renders into the settings shell.
    return <Onboarding onDone={() => undefined} />;
  }

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-[var(--surface-0)] text-[var(--ink)] select-none">
      {/* Ambient top light — the source the glass edges are lit by. */}
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-64"
        style={{
          background:
            "radial-gradient(75% 100% at 50% 0%, rgba(255,255,255,0.055), transparent 70%)",
        }}
      />

      {/* Sidebar + content */}
      <div className="relative flex min-h-0 flex-1">
        <nav className="flex w-[168px] flex-shrink-0 flex-col gap-0.5 border-r border-[var(--hairline)] p-3">
          <div className="mb-3 flex items-center gap-2 px-2">
            <span
              className="h-2 w-2 rounded-full"
              style={{
                background: "var(--ink)",
                boxShadow: "0 0 10px rgba(255,255,255,0.45)",
              }}
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
                  ? "bg-[var(--surface-2)] text-[var(--ink)] shadow-[var(--edge-light)]"
                  : "text-[var(--ink-muted)] hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
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
