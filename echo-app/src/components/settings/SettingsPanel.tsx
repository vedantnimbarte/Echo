import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Keyboard, AudioWaveform, Search } from "lucide-react";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { useRecordingStore, type RecordingMode } from "../../store/recordingStore";
import { ModelSelector } from "./ModelSelector";
import { CloudProviders } from "./CloudProviders";
import { TelemetrySettings } from "./TelemetrySettings";
import { HotkeyCapture } from "../common/HotkeyCapture";

/* ---- compact field primitives -------------------------------------------- */

const fieldCls =
  "w-full rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-[13px] text-[var(--ink)] outline-none transition focus:border-[var(--aurora-2)]/60 focus:bg-white/8";

function Section({
  title,
  desc,
  children,
}: {
  title: string;
  desc?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-xl border border-white/8 bg-white/[0.025] p-4">
      <h3 className="text-[13px] font-semibold tracking-tight text-[var(--ink)]">{title}</h3>
      {desc && <p className="mt-0.5 text-[11px] leading-snug text-[var(--ink-muted)]">{desc}</p>}
      <div className="mt-3 space-y-3">{children}</div>
    </section>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
        {label}
      </span>
      {children}
    </label>
  );
}

export function SettingsPanel() {
  const qc = useQueryClient();
  const setStoreMode = useRecordingStore((s) => s.setMode);

  /* ---- section search ---- */
  const [q, setQ] = useState("");
  const show = (terms: string[]) =>
    !q.trim() || terms.some((t) => t.toLowerCase().includes(q.trim().toLowerCase()));

  /* ---- recording mode + device ---- */
  const { data: savedMode } = useQuery({
    queryKey: ["setting", "recording_mode"],
    queryFn: () => commands.getSetting("recording_mode"),
  });
  const mode: RecordingMode = savedMode === "auto" ? "auto" : "manual";

  function changeMode(m: RecordingMode) {
    setStoreMode(m);
    void commands.setSetting("recording_mode", m).then(() => {
      qc.invalidateQueries({ queryKey: ["setting", "recording_mode"] });
    });
    void echoEvents.emitModeChanged(m); // sync the live pill
  }

  const { data: devices = [] } = useQuery({
    queryKey: ["audio-devices"],
    queryFn: commands.getAudioDevices,
  });
  const { data: savedDevice } = useQuery({
    queryKey: ["setting", "audio_device"],
    queryFn: () => commands.getSetting("audio_device"),
  });
  const setDeviceMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("audio_device", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "audio_device"] }),
  });

  /* ---- existing settings ---- */
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: historyEnabled } = useQuery({
    queryKey: ["setting", "history_enabled"],
    queryFn: () => commands.getSetting("history_enabled"),
  });
  const { data: autoInject } = useQuery({
    queryKey: ["setting", "auto_inject"],
    queryFn: () => commands.getSetting("auto_inject"),
  });
  const { data: injectDelay } = useQuery({
    queryKey: ["setting", "inject_delay_ms"],
    queryFn: () => commands.getSetting("inject_delay_ms"),
  });
  const { data: injectionMethod } = useQuery({
    queryKey: ["setting", "injection_method"],
    queryFn: () => commands.getSetting("injection_method"),
  });

  const setAutoInjectMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("auto_inject", v),
  });
  const setInjectDelayMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("inject_delay_ms", v),
  });
  const setInjectionMethodMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("injection_method", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "injection_method"] }),
  });
  // Selecting a provider must register/activate it (not just persist a string),
  // so this goes through set_asr_provider rather than set_setting.
  const setProviderMutation = useMutation({
    mutationFn: (v: string) => commands.setAsrProvider(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] }),
  });
  const setHistoryMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("history_enabled", v),
  });

  const [permissionStatus, setPermissionStatus] = useState<boolean | null>(null);
  async function checkPermission() {
    setPermissionStatus(await commands.checkAccessibilityPermission());
  }

  const { data: hotkey } = useQuery({ queryKey: ["hotkey"], queryFn: commands.getHotkey });
  const registerHotkeyMutation = useMutation({
    mutationFn: (v: string) => commands.registerHotkey(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["hotkey"] }),
  });

  const activeProvider = provider ?? "local";

  return (
    <div className="mx-auto max-w-[560px] space-y-4 p-5">
      <div className="flex items-center justify-between">
        <h2 className="text-[15px] font-semibold tracking-tight">Settings</h2>
        <button
          onClick={() => void commands.quit()}
          className="rounded-lg border border-white/10 px-2.5 py-1 text-[11px] text-[var(--ink-muted)] transition hover:border-rose-500/50 hover:bg-rose-500/10 hover:text-rose-300"
        >
          Quit Echo
        </button>
      </div>

      <div className="relative">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-faint)]" />
        <input
          value={q}
          onChange={(e) => setQ(e.target.value)}
          placeholder="Search settings…"
          className="w-full rounded-lg border border-white/10 bg-white/5 py-1.5 pl-8 pr-2.5 text-[13px] text-[var(--ink)] outline-none transition focus:border-[var(--aurora-2)]/60 focus:bg-white/8"
        />
      </div>

      {/* ---- Dictation ------------------------------------------------ */}
      {show(["dictation", "mode", "microphone", "mic", "hotkey", "push to talk", "voice", "shortcut"]) && (
        <Section
          title="Dictation"
          desc="How recording starts and which microphone to listen to."
        >
          <Field label="Mode">
            <div className="grid grid-cols-2 gap-1.5">
              {(
                [
                  {
                    id: "manual" as const,
                    Icon: Keyboard,
                    title: "Push to talk",
                    sub: "Start & stop with the hotkey",
                  },
                  {
                    id: "auto" as const,
                    Icon: AudioWaveform,
                    title: "Voice activated",
                    sub: "Records when you speak, stops on silence",
                  },
                ]
              ).map(({ id, Icon, title, sub }) => {
                const active = mode === id;
                return (
                  <button
                    key={id}
                    onClick={() => changeMode(id)}
                    className={
                      "flex flex-col gap-1 rounded-lg border p-2.5 text-left transition " +
                      (active
                        ? "border-[var(--aurora-2)]/60 bg-[var(--aurora-2)]/12"
                        : "border-white/10 bg-white/[0.02] hover:bg-white/5")
                    }
                  >
                    <span className="flex items-center gap-1.5 text-[12px] font-medium">
                      <Icon
                        className="h-3.5 w-3.5"
                        style={{ color: active ? "var(--aurora-1)" : "var(--ink-muted)" }}
                      />
                      {title}
                    </span>
                    <span className="text-[10.5px] leading-tight text-[var(--ink-muted)]">
                      {sub}
                    </span>
                  </button>
                );
              })}
            </div>
          </Field>

          <Field label="Microphone">
            <select
              className={fieldCls}
              value={savedDevice ?? ""}
              onChange={(e) => setDeviceMutation.mutate(e.target.value)}
            >
              <option value="">System default</option>
              {devices.map((d) => (
                <option key={d.name} value={d.name}>
                  {d.name}
                  {d.is_default ? " (default)" : ""}
                </option>
              ))}
            </select>
          </Field>

          <Field label="Global hotkey">
            <HotkeyCapture
              value={hotkey ?? ""}
              onChange={(accel) => registerHotkeyMutation.mutate(accel)}
            />
            {registerHotkeyMutation.isError && (
              <span className="text-[11px] text-rose-400">
                {String(registerHotkeyMutation.error)}
              </span>
            )}
          </Field>
        </Section>
      )}

      {/* ---- Transcription ------------------------------------------- */}
      {show(["transcription", "provider", "whisper", "local", "openai", "groq", "deepgram", "model", "engine", "api key", "cloud"]) && (
        <Section title="Transcription" desc="The engine that turns speech into text.">
          <Field label="Provider">
            <select
              className={fieldCls}
              value={activeProvider}
              onChange={(e) => setProviderMutation.mutate(e.target.value)}
            >
              <option value="local">Local Whisper (offline)</option>
              <option value="none">None (no transcription)</option>
              <option value="openai">OpenAI Whisper API</option>
              <option value="groq">Groq</option>
              <option value="deepgram">Deepgram (streaming)</option>
            </select>
          </Field>
          {setProviderMutation.isError && (
            <span className="text-[11px] text-rose-400">
              {String(setProviderMutation.error)}
            </span>
          )}
          {activeProvider === "local" && <ModelSelector />}
          <CloudProviders />
        </Section>
      )}

      {/* ---- Text injection ------------------------------------------ */}
      {show(["text output", "inject", "type", "keyboard", "accessibility", "delay", "focused app"]) && (
        <Section
          title="Text output"
          desc="Echo types the transcript into whatever app is focused."
        >
          <label className="flex items-center gap-2.5">
            <input
              type="checkbox"
              className="h-4 w-4 accent-[var(--aurora-2)]"
              checked={autoInject !== "false"}
              onChange={(e) => setAutoInjectMutation.mutate(e.target.checked ? "true" : "false")}
            />
            <span className="text-[12px] text-[var(--ink)]">
              Insert text into the focused app after transcription
            </span>
          </label>

          <Field label="Insert method">
            <select
              className={fieldCls + " w-48"}
              value={injectionMethod ?? "type"}
              onChange={(e) => setInjectionMethodMutation.mutate(e.target.value)}
            >
              <option value="type">Type keystrokes (universal)</option>
              <option value="paste">Paste (fast, best for long text)</option>
            </select>
          </Field>
          {injectionMethod === "paste" && (
            <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
              Paste briefly replaces your clipboard, then restores it. Some apps (e.g. terminals)
              use a different paste shortcut — switch back to typing if it doesn't land.
            </p>
          )}

          <Field label="Insert delay (ms)">
            <input
              type="number"
              min={0}
              className={fieldCls + " w-28"}
              defaultValue={injectDelay ?? "0"}
              onBlur={(e) => setInjectDelayMutation.mutate(e.target.value || "0")}
            />
          </Field>

          <div className="flex items-center gap-2.5">
            <button
              onClick={checkPermission}
              className="rounded-lg border border-white/10 px-2.5 py-1 text-[11px] text-[var(--ink-muted)] transition hover:bg-white/5 hover:text-[var(--ink)]"
            >
              Check accessibility permission
            </button>
            {permissionStatus !== null && (
              <span
                className={
                  permissionStatus ? "text-[11px] text-emerald-400" : "text-[11px] text-rose-400"
                }
              >
                {permissionStatus ? "Granted" : "Not granted"}
              </span>
            )}
          </div>
          <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
            macOS needs Accessibility permission. Linux needs <code>xdotool</code> (X11) or{" "}
            <code>ydotool</code> (Wayland). Windows works out of the box.
          </p>
        </Section>
      )}

      {/* ---- Privacy ------------------------------------------------- */}
      {show(["privacy", "telemetry", "history", "data", "save"]) && (
        <Section title="Privacy">
          <TelemetrySettings />
          <label className="flex items-center gap-2.5 border-t border-white/6 pt-3">
            <input
              type="checkbox"
              className="h-4 w-4 accent-[var(--aurora-2)]"
              checked={historyEnabled !== "false"}
              onChange={(e) => setHistoryMutation.mutate(e.target.checked ? "true" : "false")}
            />
            <span className="text-[12px] text-[var(--ink)]">Save transcription history</span>
          </label>
        </Section>
      )}
    </div>
  );
}
