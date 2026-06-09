import { useState } from "react";
import clsx from "clsx";
import { Mic, BookOpen, Clock, Settings, Puzzle } from "lucide-react";

import { useEchoEvents } from "./hooks/useEchoEvents";
import { RecordingPanel } from "./components/recording/RecordingPanel";
import { DictionaryPanel } from "./components/dictionary/DictionaryPanel";
import { HistoryPanel } from "./components/history/HistoryPanel";
import { SettingsPanel } from "./components/settings/SettingsPanel";
import { PluginsPanel } from "./components/plugins/PluginsPanel";

type Tab = "record" | "dictionary" | "history" | "plugins" | "settings";

const TABS: { id: Tab; label: string; Icon: React.ElementType }[] = [
  { id: "record", label: "Record", Icon: Mic },
  { id: "dictionary", label: "Dictionary", Icon: BookOpen },
  { id: "history", label: "History", Icon: Clock },
  { id: "plugins", label: "Plugins", Icon: Puzzle },
  { id: "settings", label: "Settings", Icon: Settings },
];

export default function App() {
  useEchoEvents();
  const [tab, setTab] = useState<Tab>("record");

  return (
    <div className="flex flex-col h-screen bg-zinc-900 text-zinc-100 select-none">
      {/* Title bar drag region */}
      <div data-tauri-drag-region className="h-8 bg-zinc-950 flex-shrink-0" />

      {/* Nav */}
      <nav className="flex border-b border-zinc-800 bg-zinc-950 px-2">
        {TABS.map(({ id, label, Icon }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={clsx(
              "flex items-center gap-2 px-4 py-2.5 text-sm transition-colors border-b-2",
              tab === id
                ? "border-violet-500 text-violet-400"
                : "border-transparent text-zinc-500 hover:text-zinc-300"
            )}
          >
            <Icon className="w-4 h-4" />
            {label}
          </button>
        ))}
      </nav>

      {/* Content */}
      <main className="flex-1 overflow-y-auto">
        {tab === "record" && <RecordingPanel />}
        {tab === "dictionary" && <DictionaryPanel />}
        {tab === "history" && <HistoryPanel />}
        {tab === "plugins" && <PluginsPanel />}
        {tab === "settings" && <SettingsPanel />}
      </main>
    </div>
  );
}
