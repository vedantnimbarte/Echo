import { useEffect, useRef, useState } from "react";
import clsx from "clsx";
import { Mic, Square, Settings, Check, AlertTriangle } from "lucide-react";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";
import { getCurrentWindow } from "@tauri-apps/api/window";

import { useRecordingStore } from "../../store/recordingStore";
import { commands } from "../../ipc/commands";
import { Waveform, type WaveMode } from "./Waveform";
import { RingMeter } from "./RingMeter";

export type PillSize = "large" | "small";

/** Movement, in px, before a press on a control counts as a drag not a click. */
const DRAG_THRESHOLD = 4;

type View = "idle" | "active" | "transcribing" | "done" | "error";

async function openSettings() {
  const wins = await getAllWebviewWindows();
  const main = wins.find((w) => w.label === "main");
  if (main) {
    await main.show();
    await main.setFocus();
  }
}

function formatElapsed(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

/**
 * The pill's state machine, shared by both variants.
 *
 * Both sizes show the same five states from the same store; only how much room
 * they have to say it differs. Keeping the logic here means a variant is purely
 * a rendering decision.
 */
function usePillState() {
  const {
    isRecording,
    speaking,
    transcribing,
    mode,
    finalTranscript,
    partialTranscript,
    error,
    setError,
    setTranscribing,
  } = useRecordingStore();

  const [elapsed, setElapsed] = useState(0);
  const [flash, setFlash] = useState(false);
  const prevFinal = useRef(finalTranscript);

  // Elapsed timer (manual sessions only).
  useEffect(() => {
    if (!isRecording) {
      setElapsed(0);
      return;
    }
    const startedAt = performance.now();
    const id = setInterval(
      () => setElapsed(Math.floor((performance.now() - startedAt) / 1000)),
      250
    );
    return () => clearInterval(id);
  }, [isRecording]);

  // Confirmation flash on each inserted transcript.
  useEffect(() => {
    if (finalTranscript && finalTranscript !== prevFinal.current) {
      setFlash(true);
      const id = setTimeout(() => setFlash(false), 1800);
      prevFinal.current = finalTranscript;
      return () => clearTimeout(id);
    }
    prevFinal.current = finalTranscript;
  }, [finalTranscript]);

  // Watchdog: never let "transcribing" stick (e.g. provider = none, no final).
  useEffect(() => {
    if (!transcribing) return;
    const id = setTimeout(() => setTranscribing(false), 6000);
    return () => clearTimeout(id);
  }, [transcribing, setTranscribing]);

  // Auto-dismiss errors.
  useEffect(() => {
    if (!error) return;
    const id = setTimeout(() => setError(null), 4500);
    return () => clearTimeout(id);
  }, [error, setError]);

  const view: View = error
    ? "error"
    : transcribing
      ? "transcribing"
      : isRecording
        ? "active"
        : flash
          ? "done"
          : "idle";

  // A "hot" indicator when actually capturing speech; calm while merely armed.
  const live = view === "transcribing" || speaking || (isRecording && mode === "manual");

  function toggle() {
    if (isRecording) {
      void commands.stopRecording();
    } else {
      // Surface capture failures (no mic, permission denied) in the pill.
      void commands.startRecording().catch((e) => setError(String(e)));
    }
  }

  function retry() {
    setError(null);
    toggle();
  }

  return {
    view,
    live,
    mode,
    isRecording,
    elapsed,
    error,
    partialTranscript,
    toggle,
    retry,
  };
}

type PillState = ReturnType<typeof usePillState>;

export function Pill({
  size,
  onDragStart,
}: {
  size: PillSize;
  /** Called when the user starts dragging, so the new spot can be remembered. */
  onDragStart?: () => void;
}) {
  const state = usePillState();

  // Set the moment a drag begins, so a click synthesized at the end of it can't
  // toggle recording. Cleared on the next press.
  const suppressClick = useRef(false);

  /**
   * Drag the window from anywhere on the pill.
   *
   * Handled here rather than with `data-tauri-drag-region`, which only matches
   * the exact element carrying the attribute — that left the waveform, the
   * status text and the transparent area around the capsule all dead to a
   * drag, so the only handle was the few pixels of shell padding.
   */
  function startDrag(e: React.MouseEvent) {
    if (e.button !== 0) return;
    suppressClick.current = false;

    const begin = () => {
      suppressClick.current = true;
      onDragStart?.();
      void getCurrentWindow().startDragging();
    };

    // Empty shell, or the transparent area around it: nothing to click here, so
    // take the grab straight away.
    if (!(e.target as Element).closest("button")) {
      begin();
      return;
    }

    // On a control, a press is still ambiguous. Refusing to drag from one would
    // leave the small variant with no handle at all — it is almost entirely a
    // single button — so wait for movement, and let a press that stays put fall
    // through to the click it was.
    const originX = e.clientX;
    const originY = e.clientY;

    const onMove = (m: MouseEvent) => {
      if (
        Math.abs(m.clientX - originX) < DRAG_THRESHOLD &&
        Math.abs(m.clientY - originY) < DRAG_THRESHOLD
      ) {
        return;
      }
      stop();
      begin();
    };
    const stop = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", stop);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", stop);
  }

  return (
    <div
      className="h-screen w-screen"
      onMouseDown={startDrag}
      onClickCapture={(e) => {
        if (!suppressClick.current) return;
        suppressClick.current = false;
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {size === "small" ? <PillSmall {...state} /> : <PillLarge {...state} />}
    </div>
  );
}

/* ---- Large ---------------------------------------------------------------- */

/**
 * The full capsule. The meter is present in every state — flat at rest, live
 * while you speak, sweeping while Whisper works — so the pill reads as one
 * instrument rather than a bar that swaps between labels. That is why there is
 * no text at rest and no keyboard hint: the line already says "ready", and it
 * says it in the same language as every other state.
 */
function PillLarge({
  view,
  live,
  mode,
  isRecording,
  elapsed,
  error,
  partialTranscript,
  toggle,
  retry,
}: PillState) {
  const waveMode: WaveMode =
    view === "transcribing" ? "transcribing" : view === "active" ? "listening" : "idle";
  // The red bloom means one thing only: the microphone is capturing. Whisper
  // working afterwards is not that, so transcribing stays colourless.
  const hot = view === "active";

  return (
    <div className="flex h-full w-full items-center justify-center overflow-hidden">
      <div
        className={clsx(
          "pill-shell animate-rise flex select-none items-center gap-1 rounded-full p-1.5",
          "cursor-grab active:cursor-grabbing",
          hot && "is-live"
        )}
        style={{ color: "var(--ink)" }}
      >
        {/* ---- Left control --------------------------------------------- */}
        {view === "error" ? (
          <button
            onClick={retry}
            aria-label="Try again"
            title="Try again"
            className="flex h-8 w-8 items-center justify-center rounded-full border border-[var(--hairline-strong)] bg-[var(--surface-3)] text-[var(--ink)] transition hover:bg-white/20"
          >
            <AlertTriangle className="h-3.5 w-3.5" />
          </button>
        ) : view === "done" ? (
          <span className="flex h-8 w-8 items-center justify-center rounded-full border border-[var(--hairline)] bg-[var(--surface-3)] text-[var(--ink)]">
            <Check className="h-4 w-4" />
          </span>
        ) : (
          <button
            onClick={toggle}
            aria-label={isRecording ? "Stop recording" : "Start recording"}
            className={clsx(
              "flex h-8 w-8 items-center justify-center rounded-full transition-all active:scale-90",
              !isRecording &&
                "bg-[var(--surface-2)] text-[var(--ink)] hover:bg-[var(--surface-3)]",
              // Live: the one place colour is spent.
              isRecording && live && "animate-rec bg-[var(--rec)] text-white",
              // Armed but not capturing — neutral, so red always means "live".
              isRecording &&
                !live &&
                "bg-[var(--surface-3)] text-[var(--ink)] ring-1 ring-[var(--hairline-strong)]"
            )}
          >
            {isRecording ? (
              <Square className="h-3 w-3 fill-current" />
            ) : (
              <Mic className="h-4 w-4" />
            )}
          </button>
        )}

        {/* ---- Center ---------------------------------------------------- */}
        <div className="flex min-w-0 items-center gap-2.5 px-2">
          {(view === "idle" || view === "active" || view === "transcribing") && (
            <Waveform mode={waveMode} />
          )}

          {view === "active" && partialTranscript && (
            // Live interim words, for providers that stream them.
            <span className="max-w-[200px] truncate text-[11px] tracking-tight text-[var(--ink-muted)]">
              {partialTranscript}
            </span>
          )}

          {view === "transcribing" && (
            <span className="text-shimmer whitespace-nowrap text-[11px] font-medium tracking-tight">
              Transcribing
            </span>
          )}

          {view === "done" && (
            <span className="whitespace-nowrap text-[12px] font-medium tracking-tight text-[var(--ink)]">
              Inserted
            </span>
          )}

          {view === "error" && (
            <span className="max-w-[220px] truncate text-[11px] font-medium tracking-tight text-[var(--ink)]">
              {error}
            </span>
          )}
        </div>

        {/* ---- Right controls ------------------------------------------- */}
        <div className="ml-auto flex items-center gap-0.5">
          {view === "active" && mode === "manual" && (
            <span className="tabular mr-1.5 text-[11px] tracking-tight text-[var(--ink-muted)]">
              {formatElapsed(elapsed)}
            </span>
          )}

          <button
            onClick={() => void openSettings()}
            aria-label="Open settings"
            className="flex h-8 w-8 items-center justify-center rounded-full text-[var(--ink-muted)] transition-colors hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          >
            <Settings className="h-[14px] w-[14px]" />
          </button>
        </div>
      </div>
    </div>
  );
}

/* ---- Small ---------------------------------------------------------------- */

/**
 * One 44px button. With no room for a bar meter, the button's own ring becomes
 * the meter (see `RingMeter`), so the small pill still answers the only
 * question that matters mid-sentence: is it hearing me?
 *
 * Settings is revealed on hover rather than shown, because a permanent second
 * button would double the footprint of a variant whose whole point is not
 * having one. The button is pinned so its centre never moves — the gear grows
 * out to the right instead of the pill re-centering under your cursor.
 */
function PillSmall({ view, live, isRecording, error, toggle, retry }: PillState) {
  const [hovered, setHovered] = useState(false);

  const ringMode =
    view === "transcribing" ? "transcribing" : view === "active" ? "listening" : "idle";

  const glyph =
    view === "error" ? (
      <AlertTriangle className="h-4 w-4" />
    ) : view === "done" ? (
      <Check className="h-4 w-4" />
    ) : isRecording ? (
      <Square className="h-3 w-3 fill-current" />
    ) : (
      <Mic className="h-4 w-4" />
    );

  const title =
    view === "error"
      ? (error ?? "Something went wrong")
      : isRecording
        ? "Stop recording"
        : "Start recording";

  return (
    <div className="relative h-full w-full select-none overflow-hidden">
      {/* Anchored so the button's centre sits at the window's centre no matter
          how wide the shell gets. */}
      <div
        className="absolute top-1/2 -translate-y-1/2"
        style={{ left: "calc(50% - 22px)" }}
        onMouseEnter={() => setHovered(true)}
        onMouseLeave={() => setHovered(false)}
      >
        <div
          className={clsx(
            "pill-shell animate-rise flex items-center rounded-full",
            "cursor-grab active:cursor-grabbing",
            view === "active" && "is-live"
          )}
          style={{ color: "var(--ink)" }}
        >
          <button
            onClick={view === "error" ? retry : toggle}
            aria-label={title}
            title={title}
            className={clsx(
              "relative flex h-11 w-11 items-center justify-center rounded-full",
              "text-[var(--ink)] transition-colors active:scale-90 hover:bg-[var(--surface-2)]"
            )}
          >
            <RingMeter mode={ringMode} live={isRecording && live} />
            {glyph}
          </button>

          {/* Reveals on hover. Width, not display, so it slides rather than
              popping — and stays out of the tab order when closed. */}
          <div
            className="overflow-hidden transition-[width] duration-[280ms] ease-out"
            style={{ width: hovered ? 40 : 0 }}
          >
            <button
              onClick={() => void openSettings()}
              aria-label="Open settings"
              tabIndex={hovered ? 0 : -1}
              className="flex h-11 w-10 items-center justify-center rounded-full pr-1 text-[var(--ink-muted)] transition-colors hover:text-[var(--ink)]"
            >
              <Settings className="h-[14px] w-[14px]" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
