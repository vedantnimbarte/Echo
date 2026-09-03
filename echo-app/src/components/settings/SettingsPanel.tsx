import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Keyboard, AudioWaveform, Search, Check as CheckIcon, X, AlertTriangle } from "lucide-react";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { useRecordingStore, type RecordingMode } from "../../store/recordingStore";
import { ModelSelector } from "./ModelSelector";
import { CloudProviders } from "./CloudProviders";
import { TelemetrySettings } from "./TelemetrySettings";
import { WakeWordSettings } from "./WakeWordSettings";
import { CommandMode } from "./CommandMode";
import { AppProfiles } from "./AppProfiles";
import { EgressLog } from "./EgressLog";
import { HotkeyCapture } from "../common/HotkeyCapture";
import type { PillSize } from "../pill/Pill";
import { Page, Group, Field, Check } from "../common/Page";

export type SettingsPage = "dictation" | "engine" | "output" | "privacy";

const PAGE_META: Record<SettingsPage, { title: string; description: string }> = {
  dictation: {
    title: "Dictation",
    description: "How recording starts, and which microphone Echo listens to.",
  },
  engine: {
    title: "Engine",
    description: "What turns your speech into words, and the models it runs on.",
  },
  output: {
    title: "Output",
    description: "Where the finished text goes, and how it gets there.",
  },
  privacy: {
    title: "Privacy",
    description: "What Echo keeps on this machine, and what it sends off it.",
  },
};

/**
 * Languages Whisper handles well, plus auto-detect. Not the full ~99-language
 * list: a picker nobody can scan is worse than a short one, and the long tail
 * is better served by pinning a code by hand if it ever comes up.
 */
const LANGUAGES: { code: string; label: string }[] = [
  { code: "auto", label: "Auto-detect" },
  { code: "en", label: "English" },
  { code: "es", label: "Spanish" },
  { code: "fr", label: "French" },
  { code: "de", label: "German" },
  { code: "it", label: "Italian" },
  { code: "pt", label: "Portuguese" },
  { code: "nl", label: "Dutch" },
  { code: "pl", label: "Polish" },
  { code: "ru", label: "Russian" },
  { code: "uk", label: "Ukrainian" },
  { code: "tr", label: "Turkish" },
  { code: "ar", label: "Arabic" },
  { code: "hi", label: "Hindi" },
  { code: "zh", label: "Chinese" },
  { code: "ja", label: "Japanese" },
  { code: "ko", label: "Korean" },
];

/** Inline problem report, in the one place the failing control lives. */
function Problem({ children }: { children: React.ReactNode }) {
  return (
    <span className="flex items-start gap-1.5 text-[11px] font-medium leading-snug text-[var(--ink)]">
      <AlertTriangle className="mt-px h-3 w-3 shrink-0" />
      {children}
    </span>
  );
}

export function SettingsPanel({ page }: { page: SettingsPage }) {
  const qc = useQueryClient();
  const setStoreMode = useRecordingStore((s) => s.setMode);

  /* ---- search ----------------------------------------------------------- */
  // Splitting settings across pages hides things by design, so search has to
  // reach across all four — otherwise finding a control means guessing which
  // page it lives on.
  const [q, setQ] = useState("");
  const query = q.trim().toLowerCase();
  const searching = query.length > 0;
  const on = (owner: SettingsPage, terms: string[]) =>
    searching ? terms.some((t) => t.includes(query)) : owner === page;
  // While searching, groups arrive out of context — say where each one lives.
  const label = (owner: SettingsPage, title: string) =>
    searching ? `${PAGE_META[owner].title} · ${title}` : title;

  /* ---- recording mode + device ------------------------------------------ */
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

  /* ---- pill ------------------------------------------------------------- */
  const { data: pillSize } = useQuery({
    queryKey: ["setting", "pill_size"],
    queryFn: () => commands.getSetting("pill_size"),
  });
  const setPillSizeMutation = useMutation({
    // The pill is a separate webview with its own store, so persisting the
    // choice isn't enough — it has to be told, the same way mode changes are.
    mutationFn: async (v: PillSize) => {
      await commands.setSetting("pill_size", v);
      await echoEvents.emitPillSizeChanged(v);
    },
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "pill_size"] }),
  });
  const activePill: PillSize = pillSize === "small" ? "small" : "large";

  /* ---- engine ----------------------------------------------------------- */
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: language } = useQuery({
    queryKey: ["setting", "language"],
    queryFn: () => commands.getSetting("language"),
  });
  const setLanguageMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("language", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "language"] }),
  });
  // Selecting a provider must register/activate it (not just persist a string),
  // so this goes through set_asr_provider rather than set_setting.
  const setProviderMutation = useMutation({
    mutationFn: (v: string) => commands.setAsrProvider(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] }),
  });

  /* ---- output ----------------------------------------------------------- */
  const { data: autoInject } = useQuery({
    queryKey: ["setting", "auto_inject"],
    queryFn: () => commands.getSetting("auto_inject"),
  });
  const { data: injectDelay } = useQuery({
    queryKey: ["setting", "inject_delay_ms"],
    queryFn: () => commands.getSetting("inject_delay_ms"),
  });
  const { data: clipboardSettle } = useQuery({
    queryKey: ["setting", "clipboard_settle_ms"],
    queryFn: () => commands.getSetting("clipboard_settle_ms"),
  });
  const setClipboardSettleMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("clipboard_settle_ms", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "clipboard_settle_ms"] }),
  });
  const { data: injectionMethod } = useQuery({
    queryKey: ["setting", "injection_method"],
    queryFn: () => commands.getSetting("injection_method"),
  });
  const setAutoInjectMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("auto_inject", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "auto_inject"] }),
  });
  const setInjectDelayMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("inject_delay_ms", v),
  });
  const setInjectionMethodMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("injection_method", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "injection_method"] }),
  });

  const [permissionStatus, setPermissionStatus] = useState<boolean | null>(null);
  async function checkPermission() {
    setPermissionStatus(await commands.checkAccessibilityPermission());
  }

  /* ---- privacy ---------------------------------------------------------- */
  const { data: historyEnabled } = useQuery({
    queryKey: ["setting", "history_enabled"],
    queryFn: () => commands.getSetting("history_enabled"),
  });
  const setHistoryMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("history_enabled", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "history_enabled"] }),
  });

  const { data: hotkey } = useQuery({ queryKey: ["hotkey"], queryFn: commands.getHotkey });
  const registerHotkeyMutation = useMutation({
    mutationFn: (v: string) => commands.registerHotkey(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["hotkey"] }),
  });

  const activeProvider = provider ?? "local";
  const meta = PAGE_META[page];

  const search = (
    <div className="relative w-[184px]">
      <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-faint)]" />
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder="Search settings"
        aria-label="Search settings"
        className="field py-1.5 pl-8 pr-2.5"
      />
    </div>
  );

  return (
    <Page
      title={searching ? "Search" : meta.title}
      description={
        searching
          ? `Everything matching “${q.trim()}”, from all four settings pages.`
          : meta.description
      }
      actions={search}
    >
      {/* ---- Dictation ---------------------------------------------------- */}
      {on("dictation", ["mode", "push to talk", "voice activated", "dictation", "recording"]) && (
        <Group
          title={label("dictation", "Mode")}
          hint="In push to talk the microphone stays open between utterances — it stops only when you press the hotkey again."
        >
          <div className="grid grid-cols-2 gap-2.5">
            {(
              [
                {
                  id: "manual" as const,
                  Icon: Keyboard,
                  title: "Push to talk",
                  sub: "Start and stop with the hotkey",
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
                  aria-pressed={active}
                  className={
                    "flex flex-col gap-1.5 rounded-xl border p-3.5 text-left transition " +
                    (active
                      ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-[var(--edge-light)]"
                      : "border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)]")
                  }
                >
                  <span className="flex items-center gap-2 text-[12.5px] font-medium">
                    <Icon
                      className="h-4 w-4"
                      style={{ color: active ? "var(--ink)" : "var(--ink-muted)" }}
                    />
                    {title}
                  </span>
                  <span className="text-[11px] leading-snug text-[var(--ink-muted)]">{sub}</span>
                </button>
              );
            })}
          </div>
        </Group>
      )}

      {on("dictation", ["pill", "size", "small", "large", "compact", "overlay", "floating", "drag", "move", "position"]) && (
        <Group
          title={label("dictation", "Pill")}
          hint="The floating control you dictate from — drag it anywhere on screen and Echo puts it back there next launch. Both sizes show the same live level: the large one along a bar, the small one around its edge."
        >
          <div className="grid grid-cols-2 gap-2.5">
            {(
              [
                {
                  id: "large" as const,
                  title: "Large",
                  sub: "Level meter, elapsed time and settings, always visible",
                  // Drawn to scale with each other, so the choice is legible
                  // before you make it.
                  glyph: "h-3.5 w-14",
                },
                {
                  id: "small" as const,
                  title: "Small",
                  sub: "Just the microphone; settings appear when you point at it",
                  glyph: "h-3.5 w-3.5",
                },
              ]
            ).map(({ id, title, sub, glyph }) => {
              const active = activePill === id;
              return (
                <button
                  key={id}
                  onClick={() => setPillSizeMutation.mutate(id)}
                  aria-pressed={active}
                  className={
                    "flex flex-col gap-2 rounded-xl border p-3.5 text-left transition " +
                    (active
                      ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-[var(--edge-light)]"
                      : "border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)]")
                  }
                >
                  <span className="flex h-4 items-center">
                    <span
                      className={
                        "rounded-full border " +
                        (active
                          ? "border-[var(--ink-muted)] "
                          : "border-[var(--hairline-strong)] ") +
                        glyph
                      }
                    />
                  </span>
                  <span className="text-[12.5px] font-medium">{title}</span>
                  <span className="text-[11px] leading-snug text-[var(--ink-muted)]">
                    {sub}
                  </span>
                </button>
              );
            })}
          </div>
        </Group>
      )}

      {on("dictation", ["microphone", "mic", "input", "device", "audio"]) && (
        <Group title={label("dictation", "Microphone")}>
          <select
            className="field"
            aria-label="Microphone"
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
        </Group>
      )}

      {on("dictation", ["hotkey", "shortcut", "keyboard", "chord", "global"]) && (
        <Group title={label("dictation", "Global hotkey")}>
          <HotkeyCapture
            value={hotkey ?? ""}
            onChange={(accel) => registerHotkeyMutation.mutate(accel)}
          />
          {registerHotkeyMutation.isError && (
            <Problem>{String(registerHotkeyMutation.error)}</Problem>
          )}
        </Group>
      )}

      {on("dictation", ["wake", "wake word", "hands free", "hey", "phrase", "always on"]) && (
        <Group
          title={label("dictation", "Wake word")}
          hint="Off by default. When on, Echo listens for the phrase and starts dictating without the hotkey."
        >
          <WakeWordSettings />
        </Group>
      )}

      {/* ---- Engine ------------------------------------------------------- */}
      {on("engine", ["provider", "engine", "whisper", "openai", "groq", "deepgram", "cloud", "offline"]) && (
        <Group title={label("engine", "Speech engine")}>
          <select
            className="field"
            aria-label="Speech engine"
            value={activeProvider}
            onChange={(e) => setProviderMutation.mutate(e.target.value)}
          >
            <option value="local">Local Whisper (offline)</option>
            <option value="none">None (no transcription)</option>
            <option value="openai">OpenAI Whisper API</option>
            <option value="groq">Groq</option>
            <option value="deepgram">Deepgram (streaming)</option>
          </select>
          {setProviderMutation.isError && <Problem>{String(setProviderMutation.error)}</Problem>}
        </Group>
      )}

      {on("engine", ["model", "models", "local", "download", "remove", "delete", "disk", "storage"]) && (
        <Group title={label("engine", "Local models")}>
          <ModelSelector />
        </Group>
      )}

      {on("engine", ["language", "auto-detect", "english", "multilingual"]) && (
        <Group
          title={label("engine", "Language")}
          hint="Auto-detect works well across a whole utterance but can guess wrong on short ones. Pinning your language is more accurate if you always dictate in it. English-only models ignore this."
        >
          <select
            className="field"
            aria-label="Language"
            value={language ?? "auto"}
            onChange={(e) => setLanguageMutation.mutate(e.target.value)}
          >
            {LANGUAGES.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
        </Group>
      )}

      {on("engine", ["api key", "key", "openai", "groq", "deepgram", "cloud", "keychain"]) && (
        <Group title={label("engine", "Cloud API keys")}>
          <CloudProviders />
        </Group>
      )}

      {on("engine", ["command", "command mode", "llm", "ollama", "rewrite", "instruction"]) && (
        <Group
          title={label("engine", "Command mode")}
          hint="Speak an instruction instead of dictating text. Off by default."
        >
          <CommandMode />
        </Group>
      )}

      {/* ---- Output ------------------------------------------------------- */}
      {on("output", ["insert", "inject", "type", "paste", "clipboard", "output", "delay"]) && (
        <Group title={label("output", "Insert into the focused app")}>
          <Check
            checked={autoInject !== "false"}
            onChange={(v) => setAutoInjectMutation.mutate(v ? "true" : "false")}
          >
            Insert the transcript as soon as it's ready
          </Check>

          <Field label="Method">
            <select
              className="field w-64"
              value={injectionMethod ?? "type"}
              onChange={(e) => setInjectionMethodMutation.mutate(e.target.value)}
            >
              <option value="type">Type keystrokes (universal)</option>
              <option value="paste">Paste (fast, best for long text)</option>
            </select>
          </Field>

          {injectionMethod === "paste" && (
            <>
              <Field label="Clipboard hold (ms)">
                <input
                  type="number"
                  min={20}
                  step={20}
                  className="field w-32"
                  defaultValue={clipboardSettle ?? "180"}
                  onBlur={(e) => setClipboardSettleMutation.mutate(e.target.value || "180")}
                />
              </Field>
              <p className="max-w-[56ch] text-[10.5px] leading-relaxed text-[var(--ink-faint)]">
                Pasting briefly replaces your clipboard, then puts it back. Raise
                the hold if text goes missing — Electron apps, terminals and
                remote desktops often need longer than the default to read it.
              </p>
            </>
          )}

          <Field label="Insert delay (ms)">
            <input
              type="number"
              min={0}
              className="field w-32"
              defaultValue={injectDelay ?? "0"}
              onBlur={(e) => setInjectDelayMutation.mutate(e.target.value || "0")}
            />
          </Field>
        </Group>
      )}

      {on("output", ["permission", "accessibility", "xdotool", "ydotool", "macos", "linux"]) && (
        <Group
          title={label("output", "Permissions")}
          hint={
            <>
              macOS needs Accessibility permission. Linux needs <code>xdotool</code> (X11) or{" "}
              <code>ydotool</code> (Wayland). Windows works out of the box.
            </>
          }
        >
          <div className="flex items-center gap-3">
            <button onClick={checkPermission} className="btn-ghost px-3 py-1.5 text-[11.5px]">
              Check permission
            </button>
            {permissionStatus !== null && (
              // Without colour the icon is what distinguishes these two states.
              <span className="flex items-center gap-1.5 text-[11.5px] font-medium text-[var(--ink)]">
                {permissionStatus ? (
                  <CheckIcon className="h-3.5 w-3.5" />
                ) : (
                  <X className="h-3.5 w-3.5" />
                )}
                {permissionStatus ? "Granted" : "Not granted"}
              </span>
            )}
          </div>
        </Group>
      )}

      {on("output", ["app", "per app", "profile", "profiles", "exclude", "override", "terminal"]) && (
        <Group
          title={label("output", "Per-app profiles")}
          hint="Override how Echo behaves in specific applications."
        >
          <AppProfiles />
        </Group>
      )}

      {/* ---- Privacy ------------------------------------------------------ */}
      {on("privacy", ["request", "network", "egress", "offline", "outbound", "privacy"]) && (
        <Group title={label("privacy", "Request log")}>
          <EgressLog />
        </Group>
      )}

      {on("privacy", ["telemetry", "usage", "events", "analytics"]) && (
        <Group title={label("privacy", "Telemetry")}>
          <TelemetrySettings />
        </Group>
      )}

      {on("privacy", ["history", "transcripts", "save", "store"]) && (
        <Group title={label("privacy", "History")}>
          <Check
            checked={historyEnabled !== "false"}
            onChange={(v) => setHistoryMutation.mutate(v ? "true" : "false")}
          >
            Keep a searchable record of what you dictated
          </Check>
        </Group>
      )}
    </Page>
  );
}
