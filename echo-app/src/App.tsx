import { useEffect, useRef, useState } from "react";
import clsx from "clsx";
import {
  BarChart3,
  BookOpen,
  SlidersHorizontal,
  Puzzle,
  History,
  Cpu,
  TextCursorInput,
  ShieldCheck,
  Info,
  Power,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useEchoEvents } from "./hooks/useEchoEvents";
import { commands } from "./ipc/commands";
import { checkForUpdate, CHECK_ON_START } from "./update";
import { echoEvents } from "./ipc/events";
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
 * (Settings), turned into words (Voice engine), delivered somewhere (Output),
 * and whatever is kept afterwards is yours to see (Privacy). Four short pages
 * instead of one long scroll — you land on the topic you came for.
 */
const SETTINGS_NAV: NavItem[] = [
  // "Voice engine", not "Engine": on its own the word could be any of the
  // machinery in here, and the one it names is the part that hears you.
  { id: "engine", label: "Voice engine", Icon: Cpu },
  { id: "output", label: "Output", Icon: TextCursorInput },
  { id: "privacy", label: "Privacy", Icon: ShieldCheck },
];

/**
 * The two pages kept out of the list above because they sit at the foot of the
 * sidebar instead — beside Quit Echo, where the things you reach for
 * occasionally live rather than the topics you move between.
 *
 * About is under Settings rather than in it: which version you are on, who
 * wrote this, and where to report that it broke are questions about the
 * application, not knobs on it.
 */
const FOOT_NAV: NavItem[] = [
  { id: "settings", label: "Settings", Icon: SlidersHorizontal },
  { id: "about", label: "About", Icon: Info },
];

/**
 * Content you accumulate by using Echo, rather than settings you choose — and
 * the top of the sidebar, because it is what you open the window to look at.
 * Configuration is the thing you do once and then leave alone, so it sits
 * underneath.
 */
const LIBRARY_NAV: NavItem[] = [
  // The page ids are the ones the code has always used and are not worth
  // churning; these are the names on the buttons. "History" says what the page
  // holds, where "Dictation" named the activity and left you to guess that the
  // record of it lived there. "Custom dictionary" separates your own words from
  // the model's vocabulary, which is what people assume "Dictionary" means.
  { id: "dictation", label: "History", Icon: History },
  { id: "insights", label: "Insights", Icon: BarChart3 },
  { id: "dictionary", label: "Custom dictionary", Icon: BookOpen },
  { id: "plugins", label: "Plugins", Icon: Puzzle },
];

const SETTINGS_IDS = [...FOOT_NAV, ...SETTINGS_NAV].map((i) => i.id);

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

/**
 * The Echo mark, at the head of the sidebar.
 *
 * The logo is a spoken spike decaying into the flat parallel lines of typed
 * output, three units wide for every one tall. Drawn whole at this size the
 * lines merge into a grey smudge, so this is the crop the app icon carries —
 * the spike end, the part that survives being small — and the wordmark beside
 * it says the name anyway.
 *
 * The mark sits in a box as wide as the collapsed rail's usable width — 59px
 * less the nav's own padding — rather than being padded to the nav icons' left
 * edge. That box's centre line is the icons' centre line, so the mark stays
 * both centred in the rail and square above the icon column however large it is
 * drawn, and the wordmark starts exactly where the nav labels do.
 */
const MARK_BOX = 35;

function Brand({ collapsed }: { collapsed: boolean }) {
  return (
    <div className="mb-4 flex items-center overflow-hidden whitespace-nowrap text-[var(--ink)]">
      <span
        style={{ width: MARK_BOX }}
        className="flex shrink-0 items-center justify-center"
      >
        <svg
          viewBox="0 0 40 40"
          className="h-[26px] w-[26px]"
          fill="none"
          stroke="currentColor"
          strokeWidth="2.6"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M2 21 C4 21 5 19.5 7 19.5 C9 19.5 9.5 22 11 22 L13 20.5 L16.5 5 L19 35 L21.5 11 L24 27 C26 19 27.5 23.5 30 20.5 C33 17.5 35 23 38 20.5" />
        </svg>
      </span>
      {/* Hidden from the accessibility tree when the rail has clipped it, so a
          screen reader isn't read a wordmark that is not on screen. */}
      <span aria-hidden={collapsed} className="display text-[19px]">
        Echo
      </span>
    </div>
  );
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
        "flex w-full items-center gap-2.5 overflow-hidden whitespace-nowrap rounded-lg px-2.5 py-[7px] text-[14.5px] tracking-tight transition-colors",
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

export default function App() {
  // The settings window observes state only — the pill owns the hotkey toggle.
  useEchoEvents();
  // History, not Settings: the window is opened to see what you dictated far
  // more often than to change how it works, and configuration is the thing you
  // do once. It is also the first item in the sidebar, so the landing page and
  // the top of the list agree. The id stays "dictation" — renaming the button
  // is not a reason to churn every reference to the page behind it.
  const [page, setPage] = useState<Page>("dictation");
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

  // Opt-out rather than opt-in: an app that types into every window is one
  // you want patched, and the check is a single request to the release feed.
  // Absent means on, so nobody has to have visited Settings for it to work.
  const { data: checkOnStart } = useQuery({
    queryKey: ["setting", CHECK_ON_START],
    queryFn: () => commands.getSetting(CHECK_ON_START),
  });

  // Once per launch, not once per change of the setting — turning the box back
  // on in Settings should not fire a check from under the user.
  const startupChecked = useRef(false);
  useEffect(() => {
    if (checkOnStart === undefined || startupChecked.current) return;
    startupChecked.current = true;
    if (checkOnStart === "false") return;
    // Silent: nobody asked, so an up-to-date answer is not worth a dialog.
    void checkForUpdate({ silent: true });
  }, [checkOnStart]);

  // The tray's "Check for Updates…" — loud, because someone asked.
  useEffect(() => {
    const unlisten = echoEvents.onCheckForUpdates(() => {
      void checkForUpdate();
    });
    return () => {
      unlisten.then((f) => f());
    };
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

      <TitleBar
        sidebar={{ collapsed, onToggle: toggleSidebar }}
        onOpenEngine={() => setPage("engine")}
      />

      <div className="relative flex min-h-0 flex-1">
        <nav
          style={{ width: collapsed ? RAIL : COLUMN }}
          className="flex flex-shrink-0 flex-col overflow-hidden border-r border-[var(--hairline)] p-3 transition-[width] duration-200 ease-out motion-reduce:transition-none"
        >
          <Brand collapsed={collapsed} />

          {/* One list. The heading and the rule that used to split these in two
              were labelling a distinction — what you look at, what you set —
              that the page names already make, and they cost a reader two stops
              on the way down seven items. */}
          <div className="flex flex-col gap-0.5">
            {[...LIBRARY_NAV, ...SETTINGS_NAV].map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                collapsed={collapsed}
                onClick={() => setPage(item.id)}
              />
            ))}
          </div>

          {/* The foot of the sidebar: the pages you adjust and read about the
              app on, and the one entry that is not a page at all. `mt-auto`
              pushes the group down however tall the nav above it happens to be;
              `pt-5` keeps them off it when the window is short enough that
              there is no slack left. */}
          <div className="mt-auto flex flex-col gap-0.5 pt-5">
            <div className="mx-2.5 mb-3 border-t border-[var(--hairline)]" />

            {FOOT_NAV.map((item) => (
              <NavButton
                key={item.id}
                item={item}
                active={page === item.id}
                collapsed={collapsed}
                onClick={() => setPage(item.id)}
              />
            ))}

            {/* Quitting is an app-level action, not a setting — it belongs to the
                window chrome rather than to whichever page you happen to be on. */}
            <button
              onClick={() => void commands.quit()}
              title={collapsed ? "Quit Echo" : undefined}
              className="flex items-center gap-2.5 overflow-hidden whitespace-nowrap rounded-lg px-2.5 py-[7px] text-[14.5px] tracking-tight text-[var(--ink-muted)] transition-colors hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
            >
              <Power className="h-[15px] w-[15px] shrink-0" />
              Quit Echo
            </button>
          </div>
        </nav>

        <main className="min-w-0 flex-1 overflow-y-auto">
          {isSettingsPage(page) && <SettingsPanel page={page} />}
          {page === "insights" && <InsightsPanel />}
          {page === "dictionary" && <DictionaryPanel />}
          {page === "dictation" && (
            <HistoryPanel onOpenInsights={() => setPage("insights")} />
          )}
          {page === "plugins" && <PluginsPanel />}
        </main>
      </div>
    </div>
  );
}
