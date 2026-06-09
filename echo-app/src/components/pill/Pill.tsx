import { useEffect, useRef, useState } from "react";
import clsx from "clsx";
import { Mic, Square, Settings, Check, AlertTriangle } from "lucide-react";
import { getAllWebviewWindows } from "@tauri-apps/api/webviewWindow";

import { useRecordingStore } from "../../store/recordingStore";
import { commands } from "../../ipc/commands";
import { Waveform, type WaveMode } from "./Waveform";

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

function prettyHotkey(raw: string): string[] {
  return raw
    .replace("CommandOrControl", "Ctrl")
    .replace("Control", "Ctrl")
    .replace("Super", "Win")
    .split("+");
}

export function Pill() {
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
  const [hotkey, setHotkey] = useState<string[]>([]);
  const prevFinal = useRef(finalTranscript);

  useEffect(() => {
    void commands.getHotkey().then((h) => h && setHotkey(prettyHotkey(h)));
  }, []);

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

  const waveMode: WaveMode =
    view === "transcribing" ? "transcribing" : view === "active" ? "listening" : "idle";

  const hot = view === "active" || view === "transcribing";
  // A "hot" left dot when actually capturing speech; calm while merely armed.
  const liveDot = view === "transcribing" || speaking || (isRecording && mode === "manual");

  function toggle() {
    if (isRecording) {
      void commands.stopRecording();
    } else {
      // Surface capture failures (no mic, permission denied) in the pill.
      void commands.startRecording().catch((e) => setError(String(e)));
    }
  }

  return (
    <div className="flex h-screen w-screen items-center justify-center overflow-hidden">
      <div
        data-tauri-drag-region
        className={clsx(
          "pill-shell animate-rise flex select-none items-center rounded-full py-1.5 pl-1.5 pr-1.5",
          hot && "is-live"
        )}
        style={{ color: "var(--ink)" }}
      >
        {/* ---- Left control --------------------------------------------- */}
        {view === "error" ? (
          <button
            onClick={() => {
              setError(null);
              toggle();
            }}
            aria-label="Retry"
            title="Retry"
            className="flex h-7 w-7 items-center justify-center rounded-full bg-rose-500/15 text-rose-400 transition hover:bg-rose-500/25"
          >
            <AlertTriangle className="h-3.5 w-3.5" />
          </button>
        ) : view === "done" ? (
          <span className="flex h-7 w-7 items-center justify-center rounded-full bg-emerald-500/15 text-emerald-400">
            <Check className="h-3.5 w-3.5" />
          </span>
        ) : (
          <button
            onClick={toggle}
            aria-label={isRecording ? "Stop" : "Start"}
            className={clsx(
              "flex h-7 w-7 items-center justify-center rounded-full transition-all active:scale-90",
              !isRecording && "bg-white/8 text-[var(--ink)] hover:bg-white/14",
              isRecording && liveDot && "animate-rec bg-[var(--rec)] text-white",
              isRecording && !liveDot &&
                "bg-[var(--aurora-2)]/25 text-[var(--aurora-1)] ring-1 ring-[var(--aurora-1)]/40"
            )}
          >
            {isRecording ? (
              <Square className="h-3 w-3 fill-current" />
            ) : (
              <Mic className="h-3.5 w-3.5" />
            )}
          </button>
        )}

        {/* ---- Center ---------------------------------------------------- */}
        <div className="flex min-w-0 items-center px-2.5">
          {view === "active" &&
            (partialTranscript ? (
              // Live interim words (streaming providers) next to the meter.
              <div className="flex max-w-[240px] items-center gap-2">
                <Waveform mode={waveMode} />
                <span className="truncate text-[11px] tracking-tight text-[var(--ink-muted)]">
                  {partialTranscript}
                </span>
              </div>
            ) : (
              <Waveform mode={waveMode} />
            ))}

          {view === "transcribing" && (
            <div className="flex items-center gap-2">
              <Waveform mode={waveMode} />
              <span className="text-shimmer whitespace-nowrap text-[11px] font-medium tracking-tight">
                Transcribing
              </span>
            </div>
          )}

          {view === "done" && (
            <span className="whitespace-nowrap text-[12px] font-medium tracking-tight text-emerald-300/90">
              Inserted
            </span>
          )}

          {view === "error" && (
            <span className="max-w-[220px] truncate text-[11px] tracking-tight text-rose-300/90">
              {error}
            </span>
          )}

          {view === "idle" && (
            <div className="flex items-center gap-2 whitespace-nowrap">
              <span className="flex items-center gap-1.5 text-[12px] font-medium tracking-tight">
                <span
                  className="h-1.5 w-1.5 rounded-full"
                  style={{ background: "var(--aurora-1)", boxShadow: "0 0 6px var(--aurora-1)" }}
                />
                {mode === "auto" ? "Voice" : "Push to talk"}
              </span>
              {hotkey.length > 0 && (
                <span className="flex items-center gap-0.5">
                  {hotkey.map((k) => (
                    <kbd
                      key={k}
                      className="rounded border border-white/10 bg-white/5 px-1 py-px text-[9px] font-medium tracking-tight text-[var(--ink-muted)]"
                    >
                      {k}
                    </kbd>
                  ))}
                </span>
              )}
            </div>
          )}
        </div>

        {/* ---- Right controls ------------------------------------------- */}
        <div className="ml-auto flex items-center gap-0.5">
          {view === "active" && mode === "manual" && (
            <span className="tabular mr-1 text-[11px] tracking-tight text-[var(--ink-muted)]">
              {formatElapsed(elapsed)}
            </span>
          )}
          {view === "active" && mode === "auto" && !speaking && (
            <span className="mr-1 text-[11px] tracking-tight text-[var(--ink-muted)]">
              Listening
            </span>
          )}

          <button
            onClick={() => void openSettings()}
            aria-label="Settings"
            className="flex h-7 w-7 items-center justify-center rounded-full text-[var(--ink-muted)] transition-colors hover:bg-white/10 hover:text-[var(--ink)]"
          >
            <Settings className="h-[13px] w-[13px]" />
          </button>
        </div>
      </div>
    </div>
  );
}
