import { useEffect, useState } from "react";
import clsx from "clsx";
import { useQuery } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { commands } from "../../ipc/commands";

/**
 * The settings window's own chrome.
 *
 * Windows and Linux draw a title bar in the OS's colours, which on a window
 * this dark reads as a strip of someone else's app stapled to the top. So the
 * native one is off (see the setup hook in lib.rs) and this stands in for it.
 *
 * macOS keeps its traffic lights — they're muscle memory there, and the window
 * is set `titleBarStyle: "Overlay"` so they float inside this same strip. The
 * left slot is padded out of their way there, and everywhere else it starts at
 * the edge.
 *
 * Two zones: what the window contains on the left, what the window does to
 * itself on the right. The brand used to sit between them; it lives at the head
 * of the sidebar now, where it reads as the app the pages belong to rather than
 * as a label on the frame. Onboarding has no sidebar to put it in, but its
 * first screen says "Welcome to Echo" in type this bar could never match.
 */

/* WKWebView is the only place traffic lights exist, so the user agent is a
   truthful test and saves a dependency on the os plugin for one boolean. */
const isMac = navigator.userAgent.includes("Macintosh");

/* Window chrome wants hairlines at 10px, not the 2px strokes the nav icons are
   drawn with — a Lucide glyph at this size reads as a button with a picture on
   it rather than as part of the frame. The sidebar toggle is drawn to the same
   rule for the same reason: it sits in the frame, so it is built like frame. */
function Glyph({ d }: { d: React.ReactNode }) {
  return (
    <svg
      viewBox="0 0 10 10"
      className="h-[10px] w-[10px]"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
      aria-hidden="true"
    >
      {d}
    </svg>
  );
}

const MINIMIZE = <line x1="0" y1="5" x2="10" y2="5" />;
const MAXIMIZE = <rect x="0.5" y="0.5" width="9" height="9" />;
const CLOSE = <path d="M0.5 0.5l9 9M9.5 0.5l-9 9" />;
const RESTORE = (
  <>
    <rect x="0.5" y="2.5" width="7" height="7" />
    <path d="M2.5 2.5V0.5h7v7h-2" />
  </>
);

/* The glyph reports the state — a pane with its column, or a pane without one —
   and the tooltip names the action. A chevron would have to do both jobs at
   once and ends up ambiguous about which it is doing. */
const PANE_WITH_COLUMN = (
  <>
    <rect x="0.5" y="0.5" width="9" height="9" rx="1.5" />
    <path d="M3.5 0.5v9" />
  </>
);
const PANE_ALONE = <rect x="0.5" y="0.5" width="9" height="9" rx="1.5" />;

function Control({
  label,
  glyph,
  onClick,
  danger,
}: {
  label: string;
  glyph: React.ReactNode;
  onClick: () => void;
  /* Close gets the brightest surface rather than the customary red. The one
     chromatic value in the app belongs to a live microphone, and closing this
     window only tucks it back into the tray — red would both overstate that and
     spend the signal that has to carry across a room. */
  danger?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      aria-label={label}
      title={label}
      className={
        "flex h-9 w-11 items-center justify-center text-[var(--ink-muted)] transition-colors hover:text-[var(--ink)] " +
        (danger ? "hover:bg-[var(--surface-3)]" : "hover:bg-[var(--surface-2)]")
      }
    >
      <Glyph d={glyph} />
    </button>
  );
}

/**
 * What is listening, in the middle of the bar.
 *
 * Two facts, split by a hairline: where the words are made, and which model
 * makes them. The dot is the one that matters — filled means the audio stays
 * on this machine, hollow means it is sent to someone else's. That is the
 * question a dictation app should answer without being asked, and it is worth
 * a permanent slot in the frame the way a document's name is in other apps.
 *
 * Colour is deliberately not the carrier. The single chromatic value in Echo
 * belongs to a live microphone, and a green/amber pair here would both borrow
 * it and imply one choice is safe and the other is a warning, which is not
 * what the user chose.
 */
function EngineTag({ onOpen }: { onOpen: () => void }) {
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: whisperModel } = useQuery({
    queryKey: ["setting", "whisper_model"],
    queryFn: () => commands.getSetting("whisper_model"),
  });
  // Same key the Engine page uses, so switching provider there updates this on
  // the next invalidation without a second path to keep in step.
  const { data: cloud } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });

  // Blank until both are in. A default of "Local" that flips to a cloud name a
  // frame later would be a lie told briefly, on exactly the point the tag
  // exists to be trusted about.
  if (provider === undefined || cloud === undefined) return null;

  const remote = cloud.find((p) => p.id === provider) ?? null;
  const where = remote ? remote.label : "Local";
  // Nothing chosen yet is base.en — the same default the model shelf lights up.
  const model = remote ? remote.model : whisperModel || "base.en";

  const title = remote
    ? `Dictation runs on ${remote.label}. Your audio leaves this machine. Open Engine to change it.`
    : `Dictation runs on this machine. Your audio stays here. Open Engine to change it.`;

  return (
    <button
      onClick={onOpen}
      title={title}
      aria-label={title}
      className="group pointer-events-auto flex max-w-[min(46vw,280px)] items-center gap-2 rounded-full border border-[var(--hairline)] py-[3px] pl-2 pr-2.5 text-[11.5px] tracking-tight text-[var(--ink-muted)] transition-colors hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-1)] hover:text-[var(--ink)]"
    >
      <svg viewBox="0 0 8 8" className="h-[7px] w-[7px] shrink-0" aria-hidden="true">
        <circle
          cx="4"
          cy="4"
          r="2.5"
          fill={remote ? "none" : "currentColor"}
          stroke="currentColor"
          strokeWidth="1"
        />
      </svg>

      <span className="truncate">{where}</span>

      {model && (
        <>
          <span
            aria-hidden
            className="h-[11px] w-px shrink-0 bg-[var(--hairline)] transition-colors group-hover:bg-[var(--hairline-strong)]"
          />
          <span className="truncate text-[var(--ink-faint)] transition-colors group-hover:text-[var(--ink-muted)]">
            {model}
          </span>
        </>
      )}
    </button>
  );
}

export function TitleBar({
  /** Omitted by windows that have no sidebar to fold — onboarding, for one. */
  sidebar,
  /** Omitted where there is no Engine page to land on, and where the engine is
      still being chosen — onboarding reporting a choice you have not made yet
      would be noise. */
  onOpenEngine,
}: {
  sidebar?: { collapsed: boolean; onToggle: () => void };
  onOpenEngine?: () => void;
} = {}) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (isMac) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    const sync = () => void win.isMaximized().then(setMaximized);
    sync();
    // Covers every route into and out of maximized — our button, a double click
    // on the drag region, Win+Up, a snap to the screen edge.
    void win.onResized(sync).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  const win = getCurrentWindow();

  return (
    <header
      data-tauri-drag-region
      className="relative z-10 flex h-9 shrink-0 items-center border-b border-[var(--hairline)]"
    >
      {/* Padded clear of the traffic lights on macOS, flush to the edge on the
          platforms that don't have any. */}
      <div className={clsx("flex items-center", isMac && "pl-[72px]")}>
        {sidebar && (
          <Control
            label={sidebar.collapsed ? "Expand sidebar" : "Collapse sidebar"}
            glyph={sidebar.collapsed ? PANE_ALONE : PANE_WITH_COLUMN}
            onClick={sidebar.onToggle}
          />
        )}
      </div>

      {/* The right zone is what the window does to itself; the tag is what it
          is doing. It sits at the head of that cluster rather than inside it —
          a gap wide enough that a click meant for the tag never lands on
          Close, and on macOS a matching inset from the edge, where there are
          no controls to sit against. */}
      <div className={clsx("ml-auto flex items-center gap-3", isMac && "pr-3")}>
        {onOpenEngine && <EngineTag onOpen={onOpenEngine} />}

        {/* The controls keep their own flush strip inside the gap: they are one
            block of window chrome, not three items in a row with the tag. */}
        <div className="flex items-center">
          {!isMac && (
            <>
              <Control
                label="Minimize"
                glyph={MINIMIZE}
                onClick={() => void win.minimize()}
              />
              <Control
                label={maximized ? "Restore down" : "Maximize"}
                glyph={maximized ? RESTORE : MAXIMIZE}
                onClick={() => void win.toggleMaximize()}
              />
              {/* Hide rather than close: the hotkey can only answer while Echo is
                  running, so the window goes back to the tray and "Quit Echo" in
                  the sidebar stays the one way out. Same thing the native close
                  button did — App.tsx intercepts that and hides too. */}
              <Control
                label="Close"
                glyph={CLOSE}
                onClick={() => void win.hide()}
                danger
              />
            </>
          )}
        </div>
      </div>
    </header>
  );
}
