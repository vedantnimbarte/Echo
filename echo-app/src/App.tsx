import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  BookOpen,
  Clock,
  Puzzle,
  Mic,
  Cpu,
  TextCursorInput,
  ShieldCheck,
  Power,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useEchoEvents } from "./hooks/useEchoEvents";
import { commands } from "./ipc/commands";
import { checkForUpdate } from "./update";
import { DictionaryPanel } from "./components/dictionary/DictionaryPanel";
import { HistoryPanel } from "./components/history/HistoryPanel";
import { SettingsPanel, type SettingsPage } from "./components/settings/SettingsPanel";
import { PluginsPanel } from "./components/plugins/PluginsPanel";
import { Onboarding } from "./components/onboarding/Onboarding";

type Page = SettingsPage | "dictionary" | "history" | "plugins";

type NavItem = { id: Page; label: string; Icon: React.ElementType };

/**
 * Settings split along the path a sentence takes through Echo: it is heard
 * (Dictation), turned into words (Engine), delivered somewhere (Output), and
 * whatever is kept afterwards is yours to see (Privacy). Four short pages
 * instead of one long scroll — you land on the topic you came for.
 */
const SETTINGS_NAV: NavItem[] = [
  { id: "dictation", label: "Dictation", Icon: Mic },
  { id: "engine", label: "Engine", Icon: Cpu },
  { id: "output", label: "Output", Icon: TextCursorInput },
  { id: "privacy", label: "Privacy", Icon: ShieldCheck },
];

/** Content you accumulate by using Echo, rather than settings you choose. */
const LIBRARY_NAV: NavItem[] = [
  { id: "dictionary", label: "Dictionary", Icon: BookOpen },
  { id: "history", label: "History", Icon: Clock },
  { id: "plugins", label: "Plugins", Icon: Puzzle },
];

const SETTINGS_IDS = SETTINGS_NAV.map((i) => i.id);

function isSettingsPage(page: Page): page is SettingsPage {
  return (SETTINGS_IDS as Page[]).includes(page);
}

function NavButton({
  item,
  active,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  onClick: () => void;
}) {
  const { label, Icon } = item;
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={clsx(
        "flex w-full items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[12.5px] tracking-tight transition-colors",
        active
          ? "bg-[var(--surface-2)] text-[var(--ink)] shadow-[var(--edge-light)]"
          : "text-[var(--ink-muted)] hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
      )}
    >
      <Icon className="h-[15px] w-[15px] shrink-0" />
      {label}
    </button>
  );
}

function NavGroup({ label }: { label: string }) {
  return (
    <span className="px-2.5 pb-1.5 pt-1 text-[10px] font-medium uppercase tracking-[0.11em] text-[var(--ink-faint)]">
      {label}
    </span>
  );
}

export default function App() {
  // The settings window observes state only — the pill owns the hotkey toggle.
  useEchoEvents();
  const [page, setPage] = useState<Page>("dictation");

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

      <div className="relative flex min-h-0 flex-1">
        <nav className="flex w-[196px] flex-shrink-0 flex-col border-r border-[var(--hairline)] p-3.5">
          <div className="mb-5 flex items-center gap-2 px-2.5 pt-1">
            <span
              className="h-2 w-2 rounded-full"
              style={{
                background: "var(--ink)",
                boxShadow: "0 0 10px rgba(255,255,255,0.45)",
              }}
            />
            <span className="text-[13px] font-semibold tracking-tight">Echo</span>
          </div>

          <div className="flex flex-col gap-0.5">
            <NavGroup label="Settings" />
            {SETTINGS_NAV.map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          <div className="mx-2.5 my-4 border-t border-[var(--hairline)]" />

          <div className="flex flex-col gap-0.5">
            {LIBRARY_NAV.map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          {/* Quitting is an app-level action, not a setting — it belongs to the
              window chrome rather than to whichever page you happen to be on. */}
          <button
            onClick={() => void commands.quit()}
            className="mt-auto flex items-center gap-2.5 rounded-lg px-2.5 py-[7px] text-[12.5px] tracking-tight text-[var(--ink-muted)] transition-colors hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
          >
            <Power className="h-[15px] w-[15px] shrink-0" />
            Quit Echo
          </button>
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto">
          {isSettingsPage(page) && <SettingsPanel page={page} />}
          {page === "dictionary" && <DictionaryPanel />}
          {page === "history" && <HistoryPanel />}
          {page === "plugins" && <PluginsPanel />}
        </main>
      </div>
    </div>
  );
}
