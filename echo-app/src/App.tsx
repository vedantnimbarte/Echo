import { useEffect, useState } from "react";
import clsx from "clsx";
import {
  BarChart3,
  BookOpen,
  SlidersHorizontal,
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
import { TitleBar } from "./components/common/TitleBar";
import { DictionaryPanel } from "./components/dictionary/DictionaryPanel";
import { HistoryPanel } from "./components/history/HistoryPanel";
import { InsightsPanel } from "./components/insights/InsightsPanel";
import { SettingsPanel, type SettingsPage } from "./components/settings/SettingsPanel";
import { PluginsPanel } from "./components/plugins/PluginsPanel";
import { Onboarding } from "./components/onboarding/Onboarding";

type Page = SettingsPage | "insights" | "dictionary" | "dictation" | "plugins";

type NavItem = { id: Page; label: string; Icon: React.ElementType };

/**
 * Settings split along the path a sentence takes through Echo: it is heard
 * (Dictation), turned into words (Engine), delivered somewhere (Output), and
 * whatever is kept afterwards is yours to see (Privacy). Four short pages
 * instead of one long scroll — you land on the topic you came for.
 */
const SETTINGS_NAV: NavItem[] = [
  { id: "settings", label: "Settings", Icon: SlidersHorizontal },
  { id: "engine", label: "Engine", Icon: Cpu },
  { id: "output", label: "Output", Icon: TextCursorInput },
  { id: "privacy", label: "Privacy", Icon: ShieldCheck },
];

/** Content you accumulate by using Echo, rather than settings you choose. */
const LIBRARY_NAV: NavItem[] = [
  { id: "insights", label: "Insights", Icon: BarChart3 },
  { id: "dictation", label: "Dictation", Icon: Mic },
  { id: "dictionary", label: "Dictionary", Icon: BookOpen },
  { id: "plugins", label: "Plugins", Icon: Puzzle },
];

const SETTINGS_IDS = SETTINGS_NAV.map((i) => i.id);

const SIDEBAR_KEY = "echo.sidebar-collapsed";

/**
 * Collapsed is a rail, not an absence: every nav item is already an icon
 * followed by its label, so narrowing the column and clipping the overflow
 * leaves the icons behind without a single conditional. 59px is what centres
 * them — 12px of nav padding, 10px of button padding, the 15px icon, and the
 * same again back out.
 */
const RAIL = 59;
const COLUMN = 188;

function isSettingsPage(page: Page): page is SettingsPage {
  return (SETTINGS_IDS as Page[]).includes(page);
}

function NavButton({
  item,
  active,
  collapsed,
  onClick,
}: {
  item: NavItem;
  active: boolean;
  collapsed: boolean;
  onClick: () => void;
}) {
  const { label, Icon } = item;
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      // Only worth a tooltip once the rail has hidden the label it would repeat.
      title={collapsed ? label : undefined}
      className={clsx(
        "flex w-full items-center gap-2.5 overflow-hidden whitespace-nowrap rounded-lg px-2.5 py-[7px] text-[12.5px] tracking-tight transition-colors",
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

function NavGroup({ label, collapsed }: { label: string; collapsed: boolean }) {
  return (
    <span
      aria-hidden={collapsed}
      className={clsx(
        "whitespace-nowrap px-2.5 pb-2 pt-1 text-[11px] font-medium text-[var(--ink-faint)] transition-opacity duration-150 motion-reduce:transition-none",
        collapsed && "opacity-0"
      )}
    >
      {label}
    </span>
  );
}

export default function App() {
  // The settings window observes state only — the pill owns the hotkey toggle.
  useEchoEvents();
  const [page, setPage] = useState<Page>("settings");
  // A view preference, not a setting — it belongs to this machine's window, so
  // it stays out of the settings database.
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(SIDEBAR_KEY) === "1"
  );

  function toggleSidebar() {
    setCollapsed((was) => {
      localStorage.setItem(SIDEBAR_KEY, was ? "0" : "1");
      return !was;
    });
  }

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
            "radial-gradient(75% 100% at 50% 0%, rgba(255,240,224,0.055), transparent 70%)",
        }}
      />

      <TitleBar sidebar={{ collapsed, onToggle: toggleSidebar }} />

      <div className="relative flex min-h-0 flex-1">
        <nav
          style={{ width: collapsed ? RAIL : COLUMN }}
          className="flex flex-shrink-0 flex-col overflow-hidden border-r border-[var(--hairline)] p-3 transition-[width] duration-200 ease-out motion-reduce:transition-none"
        >
          <div className="flex flex-col gap-0.5">
            <NavGroup label="Configure" collapsed={collapsed} />
            {SETTINGS_NAV.map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                collapsed={collapsed}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          <div className="mx-2.5 my-5 border-t border-[var(--hairline)]" />

          <div className="flex flex-col gap-0.5">
            {LIBRARY_NAV.map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                collapsed={collapsed}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          {/* Quitting is an app-level action, not a setting — it belongs to the
              window chrome rather than to whichever page you happen to be on. */}
          <button
            onClick={() => void commands.quit()}
            title={collapsed ? "Quit Echo" : undefined}
            className="mt-auto flex items-center gap-2.5 overflow-hidden whitespace-nowrap rounded-lg px-2.5 py-[7px] text-[12.5px] tracking-tight text-[var(--ink-muted)] transition-colors hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
          >
            <Power className="h-[15px] w-[15px] shrink-0" />
            Quit Echo
          </button>
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto">
          {isSettingsPage(page) && <SettingsPanel page={page} />}
          {page === "insights" && <InsightsPanel />}
          {page === "dictionary" && <DictionaryPanel />}
          {page === "dictation" && <HistoryPanel />}
          {page === "plugins" && <PluginsPanel />}
        </main>
      </div>
    </div>
  );
}
