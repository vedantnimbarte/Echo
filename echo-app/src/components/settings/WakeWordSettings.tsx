import { useEffect, useState } from "react";
import { AlertTriangle, Download, Loader2, Mic, Upload } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * Wake-word settings: turn hands-free listening on, pick or import the phrase
 * model, and tune how eagerly it fires.
 *
 * Listening is off until the user enables it here — the microphone stays idle
 * otherwise, which is the whole point of shipping this opt-in.
 */
export function WakeWordSettings() {
  const qc = useQueryClient();

  const { data: enabled } = useQuery({
    queryKey: ["setting", "wake_word_enabled"],
    queryFn: () => commands.getSetting("wake_word_enabled"),
  });
  const { data: phrases = [] } = useQuery({
    queryKey: ["wake-phrases"],
    queryFn: commands.listWakeWords,
  });
  const { data: selected } = useQuery({
    queryKey: ["setting", "wake_word_model"],
    queryFn: () => commands.getSetting("wake_word_model"),
  });
  const { data: sensitivity } = useQuery({
    queryKey: ["setting", "wake_word_sensitivity"],
    queryFn: () => commands.getSetting("wake_word_sensitivity"),
  });
  const { data: status } = useQuery({
    queryKey: ["wake-status"],
    queryFn: commands.wakeWordStatus,
    // The listener arms and disarms in the background, so poll rather than
    // guessing from the last mutation.
    refetchInterval: 3000,
  });

  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = echoEvents.onWakeModelProgress((p) => {
      setProgress(p >= 1 ? null : p);
      if (p >= 1) {
        qc.invalidateQueries({ queryKey: ["wake-phrases"] });
        qc.invalidateQueries({ queryKey: ["wake-status"] });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [qc]);

  const isOn = enabled === "true";
  const active = selected ?? "hey_jarvis";
  const threshold = Number(sensitivity ?? "0.5");
  const activePhrase = phrases.find((p) => p.id === active);

  function refresh() {
    qc.invalidateQueries({ queryKey: ["wake-phrases"] });
    qc.invalidateQueries({ queryKey: ["wake-status"] });
    qc.invalidateQueries({ queryKey: ["setting", "wake_word_enabled"] });
    qc.invalidateQueries({ queryKey: ["setting", "wake_word_model"] });
    qc.invalidateQueries({ queryKey: ["setting", "wake_word_sensitivity"] });
  }

  async function guard(fn: () => Promise<unknown>) {
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(String(e));
    }
    refresh();
  }

  async function importCustom() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "openWakeWord model", extensions: ["onnx"] }],
    });
    if (typeof picked === "string") {
      await guard(() => commands.importWakeModel(picked));
    }
  }

  return (
    <div className="space-y-3">
      <label className="flex items-start gap-2.5">
        <input
          type="checkbox"
          checked={isOn}
          onChange={(e) => guard(() => commands.setWakeWordEnabled(e.target.checked))}
          className="mt-0.5 h-3.5 w-3.5 accent-white"
        />
        <span className="text-[12px] leading-snug">
          Listen for a wake phrase
          <span className="block text-[10.5px] text-[var(--ink-muted)]">
            Echo keeps the microphone open and starts dictating when it hears the
            phrase. Everything is matched on-device — no audio leaves your machine.
            Your OS will show its microphone indicator the whole time.
          </span>
        </span>
      </label>

      <label className="block space-y-1">
        <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
          Phrase
        </span>
        <div className="flex gap-1.5">
          <select
            className="field"
            value={active}
            onChange={(e) => guard(() => commands.setWakeWordModel(e.target.value))}
          >
            {phrases.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
                {p.downloaded ? "" : " — not downloaded"}
              </option>
            ))}
            {!phrases.some((p) => p.id === active) && (
              <option value={active}>{active}</option>
            )}
          </select>

          {activePhrase && !activePhrase.downloaded && (
            <button
              onClick={() => guard(() => commands.downloadWakeModel(active))}
              disabled={progress !== null}
              className="btn-ghost shrink-0 gap-1 px-2.5 text-[12px]"
            >
              {progress !== null ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Download className="h-3.5 w-3.5" />
              )}
              {progress !== null ? `${Math.round(progress * 100)}%` : "Get"}
            </button>
          )}
        </div>
      </label>

      <label className="block space-y-1">
        <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
          Sensitivity — {threshold.toFixed(2)}
        </span>
        <input
          type="range"
          min={0.05}
          max={0.95}
          step={0.05}
          value={threshold}
          onChange={(e) =>
            guard(() => commands.setWakeWordSensitivity(Number(e.target.value)))
          }
          className="w-full accent-white"
        />
        <span className="text-[10.5px] leading-snug text-[var(--ink-muted)]">
          Lower catches the phrase more often but misfires more. Raise it if Echo
          starts recording on its own.
        </span>
      </label>

      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
          <Mic
            className="h-3.5 w-3.5"
            style={{
              color: status === "listening" ? "var(--ink)" : "var(--ink-faint)",
            }}
          />
          {status === "listening"
            ? "Listening for the wake phrase"
            : status === "model-missing"
              ? "Download the phrase model to start listening"
              : status === "idle"
                ? "Idle — dictation is using the microphone"
                : "Wake word is off"}
        </span>

        <button
          onClick={importCustom}
          className="btn-ghost shrink-0 gap-1 px-2.5 py-1 text-[11px]"
          title="Import an openWakeWord .onnx model you trained yourself"
        >
          <Upload className="h-3.5 w-3.5" />
          Import custom
        </button>
      </div>

      {error && (
        <p className="flex items-start gap-1 text-[11px] font-medium text-[var(--ink)]">
          <AlertTriangle className="mt-px h-3 w-3 shrink-0" />
          {error}
        </p>
      )}
    </div>
  );
}
