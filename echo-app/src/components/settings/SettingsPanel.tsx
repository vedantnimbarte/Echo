import { useEffect, useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import {
  Keyboard,
  AudioWaveform,
  Hand,
  Search,
  Check as CheckIcon,
  X,
  AlertTriangle,
  Laptop,
  Cloud,
} from "lucide-react";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { normalizeMode, useRecordingStore, type RecordingMode } from "../../store/recordingStore";
import { ModelSelector } from "./ModelSelector";
import { CloudProviders } from "./CloudProviders";
import { TelemetrySettings } from "./TelemetrySettings";
import { WakeWordSettings } from "./WakeWordSettings";
import { CommandMode } from "./CommandMode";
import { AppProfiles } from "./AppProfiles";
import { FixUps } from "./FixUps";
import { EgressLog } from "./EgressLog";
import { Performance } from "./Performance";
import { AudioImport } from "./AudioImport";
import { HotkeyCapture } from "../common/HotkeyCapture";
import type { PillSize } from "../pill/Pill";
import { Page, Group, Field, Check } from "../common/Page";
import { t, LOCALES, setLocale } from "../../i18n";
import { About } from "./About";

export type SettingsPage = "settings" | "engine" | "output" | "privacy" | "about";

type Lane = "local" | "cloud";

/**
 * Where a group of controls lives: a page, or a section of one.
 *
 * Written as "page.section" so a group declares its home in one string, and
 * spelled out as a union rather than left to `string` because a typo here would
 * not fail — the group would simply never appear on any tab.
 */
type Home =
  | "settings.general"
  | "settings.dictation"
  | "settings.microphone"
  | "engine.speech"
  | "engine.tools"
  | "engine.advanced"
  | "output.insert"
  | "output.formatting"
  | "output.apps"
  | "output.advanced"
  | "privacy"
  | "about";

type Tab = {
  id: Home;
  label: string;
  /** Sections that only exist for one engine lane say so, rather than opening empty. */
  when?: (lane: Lane) => boolean;
};

/**
 * Sub-navigation for the settings pages.
 *
 * Four pages was the right split — a sentence is heard, turned into words,
 * delivered somewhere, and whatever is kept afterwards is yours — but each page
 * had grown to seven or eight groups, which is more than anyone scans. So a
 * page now names its own sections, and where a page has knobs you touch once —
 * usually because something went wrong — they are gathered under Advanced at
 * the end of it. Settings has no Advanced: the only rare thing on it is a
 * microphone detail, and one select does not make a section. Naming it for what
 * it holds is what makes it findable, which was the whole point.
 *
 * Section ids are unique across pages, which is the whole reset mechanism: a
 * tab picked on one page stops matching when you move to the next, and that
 * page's first tab takes over. No effect to keep in step.
 *
 * Privacy and About have no sections. Four short groups do not need splitting,
 * and a strip that appears carrying two entries is noise rather than structure.
 */
const SETTINGS_TABS: Partial<Record<SettingsPage, Tab[]>> = {
  settings: [
    { id: "settings.general", label: "General" },
    { id: "settings.dictation", label: "Dictation" },
    { id: "settings.microphone", label: "Microphone" },
  ],
  engine: [
    { id: "engine.speech", label: "Speech" },
    { id: "engine.tools", label: "Tools" },
    // GPU and thread counts belong to the offline engine; on cloud the tab
    // would open on nothing at all.
    { id: "engine.advanced", label: "Advanced", when: (lane) => lane === "local" },
  ],
  output: [
    { id: "output.insert", label: "Insert" },
    { id: "output.formatting", label: "Formatting" },
    { id: "output.apps", label: "Apps" },
    { id: "output.advanced", label: "Advanced" },
  ],
};

// Read through `t` at call time rather than baked into a constant, so a
// language change takes effect on the next render instead of the next launch.
const pageMeta = (page: SettingsPage) => ({
  title: t(`settings.${page}.title`),
  description: t(`settings.${page}.description`),
});

/** Inline problem report, in the one place the failing control lives. */
function Problem({ children }: { children: React.ReactNode }) {
  return (
    <span className="flex items-start gap-1.5 text-[13px] font-medium leading-snug text-[var(--ink)]">
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

  /* ---- which section is open -------------------------------------------- */
  // The tab the user last clicked, which may belong to a page they have since
  // left. Resolved against the current page's sections further down, once the
  // engine lane is known.
  const [picked, setPicked] = useState<Home | null>(null);

  /* ---- launch at login ---------------------------------------------------- */
  // The OS owns this registration, so it is read back from the OS rather than
  // stored in echo.db — a user who removes the login item with their own tools
  // must not be shown a toggle that still says "on".
  const { data: autostart = false } = useQuery({
    queryKey: ["autostart"],
    queryFn: commands.getAutostart,
  });
  const setAutostartMutation = useMutation({
    mutationFn: commands.setAutostart,
    // Refetch either way: on failure the OS state is whatever it already was,
    // and the checkbox has to snap back to it rather than show the click.
    onSettled: () => qc.invalidateQueries({ queryKey: ["autostart"] }),
  });

  /* ---- recording mode + device ------------------------------------------ */
  const { data: savedMode } = useQuery({
    queryKey: ["setting", "recording_mode"],
    queryFn: () => commands.getSetting("recording_mode"),
  });
  const mode = normalizeMode(savedMode);

  // Not setSetting: hold-to-talk needs the hotkey's release as well as its
  // press, so the backend rebinds the shortcut when the mode changes.
  const setModeMutation = useMutation({
    mutationFn: (m: RecordingMode) => commands.setRecordingMode(m),
    onSuccess: (_result, m) => {
      qc.invalidateQueries({ queryKey: ["setting", "recording_mode"] });
      void echoEvents.emitModeChanged(m); // sync the live pill
    },
  });

  function changeMode(m: RecordingMode) {
    setStoreMode(m);
    setModeMutation.mutate(m);
  }

  // Language and microphone can also be set from the tray menu, which this
  // window cannot see happen — so it is told, and re-reads just that key.
  useEffect(() => {
    const unlisten = echoEvents.onSettingChanged((key) =>
      qc.invalidateQueries({ queryKey: ["setting", key] })
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, [qc]);

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
  const activePill: PillSize =
    pillSize === "small" || pillSize === "line" ? pillSize : "large";

  /* ---- engine ----------------------------------------------------------- */
  const { data: cloudProviders } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  // The list itself lives in Rust: the tray menu renders the same one.
  const { data: languages = [] } = useQuery({
    queryKey: ["dictation-languages"],
    queryFn: commands.dictationLanguages,
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
  /** Which half of the engine page is showing, once someone has said. */
  const [laneChoice, setLaneChoice] = useState<"local" | "cloud" | null>(null);

  /* ---- output ----------------------------------------------------------- */
  const { data: autoInject } = useQuery({
    queryKey: ["setting", "auto_inject"],
    queryFn: () => commands.getSetting("auto_inject"),
  });
  const { data: injectDelay } = useQuery({
    queryKey: ["setting", "inject_delay_ms"],
    queryFn: () => commands.getSetting("inject_delay_ms"),
  });
  const { data: streamPartials } = useQuery({
    queryKey: ["setting", "stream_partials"],
    queryFn: () => commands.getSetting("stream_partials"),
  });
  const { data: autoEdit } = useQuery({
    queryKey: ["setting", "auto_edit"],
    queryFn: () => commands.getSetting("auto_edit"),
  });
  const { data: autoEditLlm } = useQuery({
    queryKey: ["setting", "auto_edit_llm"],
    queryFn: () => commands.getSetting("auto_edit_llm"),
  });
  // Read here rather than inside ModelSelector: the language picker needs it to
  // spot a combination that silently produces English whatever you choose.
  const { data: whisperModel } = useQuery({
    queryKey: ["setting", "whisper_model"],
    queryFn: () => commands.getSetting("whisper_model"),
  });
  const { data: spokenPunctuation } = useQuery({
    queryKey: ["setting", "spoken_punctuation"],
    queryFn: () => commands.getSetting("spoken_punctuation"),
  });
  const { data: formatNumbers } = useQuery({
    queryKey: ["setting", "format_numbers"],
    queryFn: () => commands.getSetting("format_numbers"),
  });
  const { data: formatTidy } = useQuery({
    queryKey: ["setting", "format_tidy"],
    queryFn: () => commands.getSetting("format_tidy"),
  });
  const { data: blockSecure } = useQuery({
    queryKey: ["setting", "block_secure_fields"],
    queryFn: () => commands.getSetting("block_secure_fields"),
  });
  const { data: secureDetection } = useQuery({
    queryKey: ["secure-field-detection"],
    queryFn: commands.secureFieldDetection,
  });
  const { data: punctuationLanguages = [] } = useQuery({
    queryKey: ["spoken-punctuation-languages"],
    queryFn: commands.spokenPunctuationLanguages,
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
  // Unset means typing, which is what `resolve_delivery` decides on the Rust
  // side too — so this is the method actually running, not just the one the
  // picker happens to show.
  const method = injectionMethod ?? "type";
  // Auto pastes as readily as paste does — anything with a line break, and
  // anything long — so the clipboard knob belongs to both. Showing it only on
  // "paste" hid it from exactly the people whose text was going missing.
  const pastes = method === "paste" || method === "auto";
  const setAutoInjectMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("auto_inject", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "auto_inject"] }),
  });
  const setInjectDelayMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("inject_delay_ms", v),
  });
  const setStreamPartialsMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("stream_partials", v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", "stream_partials"] }),
  });
  // One mutation for the whole formatting group: the three stages differ only
  // in which key they write, and three near-identical hooks would say nothing
  // extra.
  const setFormatSetting = useMutation({
    mutationFn: ({ key, value }: { key: string; value: string }) =>
      commands.setSetting(key, value),
    onSuccess: (_r, { key }) => qc.invalidateQueries({ queryKey: ["setting", key] }),
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

  // Three settings below are plain string reads and writes; the shape was
  // already repeated four times above, so it earns a helper here rather than
  // another four copies.
  function useStringSetting(key: string) {
    const { data } = useQuery({
      queryKey: ["setting", key],
      queryFn: () => commands.getSetting(key),
    });
    const mutation = useMutation({
      mutationFn: (v: string) => commands.setSetting(key, v),
      onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", key] }),
    });
    return [data, (v: string) => mutation.mutate(v)] as const;
  }

  const [warmMic, setWarmMic] = useStringSetting("warm_mic");
  const [vadEngine, setVadEngine] = useStringSetting("vad_engine");
  // `undefined` while loading — only an explicit `false` means "not available",
  // so the picker does not flash a warning on the way in.
  const { data: sileroReady } = useQuery({
    queryKey: ["silero-available"],
    queryFn: commands.sileroAvailable,
  });

  const [soundCues, setSoundCues] = useStringSetting("sound_cues");
  const [uiLanguage, setUiLanguageSetting] = useStringSetting("ui_language");
  // Applied immediately as well as persisted: the whole window is already
  // mounted, so waiting for a restart to see the change would be baffling.
  const setUiLanguage = (v: string) => {
    setLocale(v);
    setUiLanguageSetting(v);
  };
  const [autoLearn, setAutoLearn] = useStringSetting("auto_learn");
  const [retention, setRetention] = useStringSetting("history_retention_days");

  const { data: hotkey } = useQuery({ queryKey: ["hotkey"], queryFn: commands.getHotkey });
  const { data: hotkeySupport } = useQuery({
    queryKey: ["hotkey-support"],
    queryFn: commands.hotkeySupport,
  });
  const registerHotkeyMutation = useMutation({
    mutationFn: (v: string) => commands.registerHotkey(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["hotkey"] }),
  });

  const activeProvider = provider ?? "local";
  // The provider in use, when it is a cloud one — also what decides which lane
  // the page opens on.
  const activeCloud = (cloudProviders ?? []).find((p) => p.id === activeProvider) ?? null;
  // Derived rather than synced: an explicit click wins, and until there is one
  // the lane follows whatever is actually running. No effect to keep in step.
  const lane = laneChoice ?? (activeCloud ? "cloud" : "local");

  function chooseLane(next: "local" | "cloud") {
    setLaneChoice(next);
    // Picking "local" is the whole decision, so it commits. Picking "cloud"
    // isn't: which provider is still unanswered, and pointing dictation at one
    // without a key would break it mid-setup. The list below commits that.
    if (next === "local" && activeProvider !== "local") setProviderMutation.mutate("local");
  }

  /* ---- sections ---------------------------------------------------------- */
  // Audio a crash interrupted waits under Tools, and nobody would think to look
  // on a tab for something they did not know survived — so when there is some,
  // that is the tab the Engine page opens on. The same query AudioImport runs,
  // shared from the cache rather than asked twice.
  const { data: recovered = [] } = useQuery({
    queryKey: ["recovered-recordings"],
    queryFn: commands.recoveredRecordings,
  });

  const tabs = (SETTINGS_TABS[page] ?? []).filter((section) => section.when?.(lane) ?? true);
  // A page with no sections is its own home, so Privacy and About match on the
  // page id and need no special case below.
  const tab: Home =
    tabs.find((section) => section.id === picked)?.id ??
    (page === "engine" && recovered.length > 0 ? "engine.tools" : (tabs[0]?.id ?? page));

  // Search reaches across every page and section — splitting settings up hides
  // things by design, and without this, finding a control means guessing twice.
  const on = (owner: Home, terms: string[]) =>
    searching ? terms.some((term) => term.includes(query)) : owner === tab;
  // While searching, groups arrive out of context — say where each one lives,
  // which is now a page and the section inside it.
  const label = (owner: Home, title: string) => {
    if (!searching) return title;
    const [owning] = owner.split(".") as [SettingsPage];
    const section = SETTINGS_TABS[owning]?.find((s) => s.id === owner);
    return [pageMeta(owning).title, section?.label, title].filter(Boolean).join(" · ");
  };

  const meta = pageMeta(page);

  const search = (
    <div className="relative w-[184px]">
      <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-faint)]" />
      <input
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={t("settings.search")}
        aria-label={t("settings.search")}
        className="field py-1.5 pl-8 pr-2.5"
      />
    </div>
  );

  // Underlined rather than the sidebar's filled pill: this switches the body of
  // one page, and borrowing the shape that means "which page" would say the
  // wrong thing twice on the same screen.
  const sections = tabs.length > 1 && (
    <nav
      aria-label={`${meta.title} sections`}
      className="mb-7 flex gap-5 border-b border-[var(--hairline)]"
    >
      {tabs.map((section) => {
        const open = section.id === tab;
        return (
          <button
            key={section.id}
            onClick={() => setPicked(section.id)}
            aria-current={open ? "true" : undefined}
            className={
              "-mb-px border-b-2 pb-2.5 text-[14px] tracking-tight transition-colors " +
              (open
                ? "border-[var(--ink)] font-medium text-[var(--ink)]"
                : "border-transparent text-[var(--ink-muted)] hover:text-[var(--ink)]")
            }
          >
            {section.label}
          </button>
        );
      })}
    </nav>
  );

  return (
    <Page
      title={searching ? "Search" : meta.title}
      description={
        searching
          ? `Everything matching “${q.trim()}”, from every page of this window.`
          : meta.description
      }
      actions={search}
      // A search is already showing every section at once, so the strip would
      // be offering to narrow to one of them and then not doing it.
      tabs={searching ? undefined : sections || undefined}
    >
      {/* ---- Settings · General ------------------------------------------ */}

      {on("settings.general", ["pill", "size", "small", "large", "minimal", "line", "capsule", "compact", "overlay", "floating", "drag", "move", "position"]) && (
        <Group
          title={label("settings.general", "Pill")}
          hint="The floating control you dictate from — drag it anywhere on screen and Echo puts it back there next launch. All three show the same live level, with less and less of the pill around it: along a bar, around the button's edge, or inside a capsule barely bigger than the meter."
        >
          <div className="grid grid-cols-3 gap-2.5">
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
                {
                  id: "line" as const,
                  title: "Minimal",
                  sub: "A bare capsule until you speak; controls appear when you point at it",
                  glyph: "h-1.5 w-8",
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
                  <span className="text-[14.5px] font-medium">{title}</span>
                  <span className="text-[13px] leading-snug text-[var(--ink-muted)]">
                    {sub}
                  </span>
                </button>
              );
            })}
          </div>
        </Group>
      )}


      {on("settings.general", ["launch", "login", "startup", "start", "boot", "autostart", "auto-start", "background", "tray", "quick access", "notification area", "menu bar"]) && (
        <Group
          title={label("settings.general", "Starting Echo")}
          hint="Echo lives in the tray — the notification area on Windows, the menu bar on macOS, the status area on Linux. Click it to reach these settings or to quit. A hotkey can only answer if Echo is already running, so starting it at login is what makes it feel like part of the keyboard rather than an app you remember to open."
        >
          <Check
            checked={autostart}
            onChange={(v) => setAutostartMutation.mutate(v)}
          >
            Start Echo when I log in
          </Check>
          {setAutostartMutation.isError && (
            <Problem>
              {(setAutostartMutation.error as Error).message} — on a managed or
              locked-down machine this is set by whoever administers it.
            </Problem>
          )}
        </Group>
      )}


      {on("settings.general", ["language", "interface", "translation", "locale", "english", "español", "deutsch", "français"]) && (
        <Group
          title={label("settings.general", "Interface language")}
          hint="This is the language Echo's own buttons and labels use. It has no effect on which language it transcribes — that is set under Engine."
        >
          {/* No Field label: the group is already called Interface language,
              and repeating it above the select said the same word twice. */}
          <select
            className="field w-full"
            aria-label={t("settings.language")}
            value={uiLanguage ?? "auto"}
            onChange={(e) => setUiLanguage(e.target.value)}
          >
            <option value="auto">{t("settings.language.auto")}</option>
            {Object.entries(LOCALES).map(([code, { label: name }]) => (
              <option key={code} value={code}>
                {name}
              </option>
            ))}
          </select>
        </Group>
      )}


      {on("settings.general", ["sound", "sounds", "cue", "cues", "tone", "beep", "audio feedback", "chime"]) && (
        <Group
          title={label("settings.general", "Sound")}
          hint="A short rising tone when Echo starts listening and a falling one when it stops. Useful when the pill is behind the window you are dictating into."
        >
          <Check
            checked={soundCues === "true"}
            onChange={(v) => setSoundCues(v ? "true" : "false")}
          >
            Play a sound when recording starts and stops
          </Check>
        </Group>
      )}


      {/* ---- Settings · Dictation ---------------------------------------- */}

      {on("settings.dictation", ["mode", "push to talk", "voice activated", "dictation", "recording"]) && (
        <Group
          title={label("settings.dictation", "Mode")}
          hint="Hold to talk waits a beat before opening the microphone, so a shortcut like Ctrl still works in the combinations you type. Tap to toggle leaves the microphone open until you press the hotkey again."
        >
          <div className="grid grid-cols-3 gap-2.5">
            {(
              [
                {
                  id: "hold" as const,
                  Icon: Hand,
                  title: "Hold to talk",
                  sub: "Records while you hold the hotkey",
                },
                {
                  id: "toggle" as const,
                  Icon: Keyboard,
                  title: "Tap to toggle",
                  sub: "Tap to start, tap again to stop",
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
                  <span className="flex items-center gap-2 text-[14.5px] font-medium">
                    <Icon
                      className="h-4 w-4"
                      style={{ color: active ? "var(--ink)" : "var(--ink-muted)" }}
                    />
                    {title}
                  </span>
                  <span className="text-[13px] leading-snug text-[var(--ink-muted)]">{sub}</span>
                </button>
              );
            })}
          </div>
        </Group>
      )}


      {on("settings.dictation", ["hotkey", "shortcut", "keyboard", "chord", "global", "ctrl", "alt", "shift", "modifier"]) && (
        <Group
          title={label("settings.dictation", "Global hotkey")}
          hint="A modifier on its own works too — tap Ctrl, Alt or Shift and release it without pressing anything else. Held as part of a combination it behaves normally, so Ctrl+C is untouched. Fn can't be used: your keyboard handles it in firmware and the key never reaches Echo."
        >
          <HotkeyCapture
            value={hotkey ?? ""}
            onChange={(accel) => registerHotkeyMutation.mutate(accel)}
          />
          {registerHotkeyMutation.isError && (
            <Problem>{String(registerHotkeyMutation.error)}</Problem>
          )}
          {/* On Wayland the shortcut registers without complaining and then
              never fires, so the only way the user finds out is if we say so. */}
          {hotkeySupport && hotkeySupport.advice !== "" && (
            <Problem>{hotkeySupport.advice}</Problem>
          )}
        </Group>
      )}


      {on("settings.dictation", ["wake", "wake word", "hands free", "hey", "phrase", "always on"]) && (
        <Group
          title={label("settings.dictation", "Wake word")}
          hint="Off by default. When on, Echo listens for the phrase and starts dictating without the hotkey."
        >
          <WakeWordSettings />
        </Group>
      )}


      {/* ---- Settings · Microphone --------------------------------------- */}
      {on("settings.microphone", [
        "microphone", "mic", "input", "device", "audio", "warm", "ready",
        "responsiveness",
      ]) && (
        // Not "Microphone": the tab already says that, and a group repeating
        // its own tab's name reads as a heading that forgot what it was for.
        <Group title={label("settings.microphone", "Input device")}>
          {/* No Field label, as under Interface language: the group is already
              called Input device and the select is the first thing under it.
              The control keeps the name for anyone reading by screen reader. */}
          <select
            className="field w-full"
            aria-label="Input device"
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

          {/* Moved here from Performance, which only renders on the local lane
              — this is about opening the audio device and has nothing to do
              with where the words are transcribed. */}
          <Check
            checked={warmMic !== "false"}
            hint={
              <>
                Keeping the microphone open for a few seconds after you stop lets
                the next sentence start instantly, and captures the moment just
                before you press the key — so a word begun early is not cut off.
                While it is open, your system will show the microphone as in use.
              </>
            }
            onChange={(v) => setWarmMic(v ? "true" : "false")}
          >
            Keep the microphone ready between dictations
          </Check>
        </Group>
      )}

            {on("settings.microphone", [
        "vad", "voice activity", "speech detection", "silero", "energy", "noise",
        "keyboard noise", "detector", "cut off", "silence",
      ]) && (
        <Group
          title={label("settings.microphone", "Speech detection")}
          hint="What decides you have started and stopped talking. The neural detector ignores keyboard clatter and fans; the simple one only measures loudness, which is worth trying if speech is being cut off or a noisy room keeps it awake."
        >
          {/* No Field label: the group is already called Speech detection and
              the select is the only thing under it, so a label would say the
              same words twice. The control keeps the name for screen readers. */}
          <select
            className="field w-full"
            aria-label="Speech detection"
            value={sileroReady === false ? "energy" : (vadEngine ?? "silero")}
            disabled={sileroReady === false}
            onChange={(e) => setVadEngine(e.target.value)}
          >
            <option value="silero">Neural — ignores background noise</option>
            <option value="energy">Simple — loudness only</option>
          </select>
          {/* The setting is honoured only when the model is there, so say so
              rather than leaving a picker that quietly does nothing. */}
          {sileroReady === false && (
            <Problem>
              The neural model didn’t load on this machine, so Echo is using the
              simple detector.
            </Problem>
          )}
        </Group>
      )}

      {/* ---- About -------------------------------------------------------- */}

      {on("about", [
        "about", "version", "update", "updates", "release", "upgrade", "new version",
        "auto-update", "issue", "bug", "report", "github", "source", "open source",
        "licence", "license", "mit", "contribute", "contributing", "star", "diagnostics",
        "feature request",
      ]) && <About label={(title) => label("about", title)} />}


      {/* ---- Engine · Speech --------------------------------------------- */}

      {on("engine.speech", ["provider", "engine", "whisper", "openai", "groq", "deepgram", "cloud", "offline", "local", "online", "off", "no transcription"]) && (
        <Group title={label("engine.speech", "Speech engine")}>
          {/* Two lanes rather than one list of eleven. The question is never
              "which of these names" — it is whether your voice stays on this
              machine, and that answer decides everything under it. Each card
              carries what is actually running, which the old dropdown could
              only say by being open. */}
          <div className="grid grid-cols-2 gap-2.5">
            {(
              [
                {
                  id: "local" as const,
                  Icon: Laptop,
                  title: "On this machine",
                  sub: "Whisper runs offline. Audio never leaves your computer.",
                  running:
                    activeProvider === "local"
                      ? `Whisper ${whisperModel || "base.en"}`
                      : null,
                },
                {
                  id: "cloud" as const,
                  Icon: Cloud,
                  title: "A cloud provider",
                  sub: "Faster and more accurate. Audio is sent to the provider you choose.",
                  running: activeCloud ? `${activeCloud.label}, ${activeCloud.model}` : null,
                },
              ]
            ).map(({ id, Icon, title, sub, running }) => {
              const selected = lane === id;
              return (
                <button
                  key={id}
                  onClick={() => chooseLane(id)}
                  aria-pressed={selected}
                  className={
                    "flex flex-col gap-2 rounded-xl border p-4 text-left transition " +
                    (selected
                      ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-[var(--edge-light)]"
                      : "border-[var(--hairline)] bg-[var(--surface-1)] hover:bg-[var(--surface-2)]")
                  }
                >
                  <Icon
                    className="h-4 w-4"
                    style={{ color: selected ? "var(--ink)" : "var(--ink-muted)" }}
                  />
                  <span className="text-[14.5px] font-medium">{title}</span>
                  <span className="text-[13px] leading-relaxed text-[var(--ink-muted)]">
                    {sub}
                  </span>
                  <span className="mt-1 text-[13px] text-[var(--ink-faint)]">
                    {running ?? "Not in use"}
                  </span>
                </button>
              );
            })}
          </div>

          {activeProvider === "none" ? (
            <p className="text-[13px] text-[var(--ink-muted)]">
              Transcription is off — Echo records nothing. Pick an engine above
              to turn it back on.
            </p>
          ) : (
            <button
              onClick={() => setProviderMutation.mutate("none")}
              className="text-[13px] text-[var(--ink-faint)] underline-offset-2 hover:text-[var(--ink)] hover:underline"
            >
              Turn transcription off
            </button>
          )}
          {setProviderMutation.isError && <Problem>{String(setProviderMutation.error)}</Problem>}
        </Group>
      )}

      {/* Models and compute belong to the offline engine; on cloud they would
          be controls for something that isn't running. Search reaches across
          pages, so a search still shows them. */}

      {on("engine.speech", ["model", "models", "local", "download", "remove", "delete", "disk", "storage"]) &&
        (searching || lane === "local") && (
          <Group title={label("engine.speech", "Local models")}>
            <ModelSelector />
          </Group>
        )}


      {on("engine.speech", ["api key", "key", "openai", "groq", "deepgram", "cloud", "keychain", "provider", "azure", "google", "mistral", "elevenlabs", "assemblyai", "speechmatics"]) &&
        (searching || lane === "cloud") && (
          <Group
            title={label("engine.speech", "Cloud provider")}
            hint="Keys are stored in your operating system's keychain, not in Echo's database. Audio for the provider you choose is sent to it as you speak; everything else stays on this machine."
          >
            <CloudProviders />
          </Group>
        )}


      {on("engine.speech", ["language", "auto-detect", "english", "multilingual"]) && (
        <Group
          title={label("engine.speech", "Language")}
          hint="Auto-detect works well across a whole utterance but can guess wrong on short ones. Pinning your language is more accurate if you always dictate in it. English-only models ignore this."
        >
          <select
            className="field"
            aria-label="Language"
            value={language ?? "auto"}
            onChange={(e) => setLanguageMutation.mutate(e.target.value)}
          >
            {languages.map((l) => (
              <option key={l.code} value={l.code}>
                {l.label}
              </option>
            ))}
          </select>
          {/* This combination produces English no matter what is picked here,
              and nothing used to say so — the transcript just came back in the
              wrong language and looked like the model being bad at yours. */}
          {whisperModel?.endsWith(".en") &&
            language !== undefined &&
            language !== null &&
            language !== "auto" &&
            language !== "en" && (
              <Problem>
                The <code>{whisperModel}</code> model is English-only, so it will
                transcribe as English whatever you choose here. Pick a
                multilingual model under Local models — the ones without{" "}
                <code>.en</code> in the name.
              </Problem>
            )}
        </Group>
      )}


      {/* ---- Engine · Tools ---------------------------------------------- */}

      {on("engine.tools", ["import", "file", "audio file", "recording", "mp3", "wav", "transcribe file", "voice memo"]) && (
        <Group
          title={label("engine.tools", "Transcribe a file")}
          hint="Uses the offline engine and the model selected above, so nothing is uploaded."
        >
          <AudioImport />
        </Group>
      )}


      {on("engine.tools", ["command", "command mode", "llm", "ollama", "rewrite", "instruction"]) && (
        <Group
          title={label("engine.tools", "Command mode")}
          hint="Speak an instruction instead of dictating text. Off by default."
        >
          <CommandMode />
        </Group>
      )}


      {/* ---- Engine · Advanced ------------------------------------------- */}

      {on("engine.advanced", ["gpu", "cuda", "nvidia", "metal", "acceleration", "accelerated", "threads", "cpu", "performance", "speed", "slow", "warm", "microphone"]) &&
        (searching || lane === "local") && <Performance />}


      {/* ---- Output · Insert --------------------------------------------- */}

      {on("output.insert", ["insert", "inject", "type", "paste", "clipboard", "output", "method"]) && (
        <Group title={label("output.insert", "Insert into the focused app")}>
          <Check
            checked={autoInject !== "false"}
            onChange={(v) => setAutoInjectMutation.mutate(v ? "true" : "false")}
          >
            Insert the transcript as soon as it’s ready
          </Check>

          <Field
            label="Method"
            hint={
              <>
                <p>
                  Typing works everywhere but is slow on long text. Pasting is
                  fast, and briefly replaces your clipboard before putting it
                  back.
                </p>
                <p className="mt-2">
                  Auto pastes anything with a line break or longer than about
                  160 characters, and types the rest. Line breaks are why this
                  matters — typed as keystrokes they become Return, which submits
                  a chat box instead of breaking the line.
                </p>
              </>
            }
          >
            <select
              className="field w-64"
              value={method}
              onChange={(e) => setInjectionMethodMutation.mutate(e.target.value)}
            >
              <option value="type">Type keystrokes (universal)</option>
              <option value="paste">Paste (fast, best for long text)</option>
              <option value="auto">Auto — type short text, paste long</option>
            </select>
          </Field>
        </Group>
      )}

      {on("output.insert", ["live", "stream", "partial", "as you speak", "realtime", "real time"]) && (
        <Group
          title={label("output.insert", "Live text")}
          hint="Off by default, and worth understanding before you turn it on: streaming rewrites text inside the focused app as the decoder revises itself, and it cannot see you typing into the same field at the same time. Turn it on per app under Per-app profiles. Each update re-decodes everything you have said so far, so without GPU acceleration the words arrive roughly every two seconds rather than keeping pace with your voice — run `echo --benchmark` to see where your machine lands."
        >
          <Check
            checked={streamPartials === "true"}
            onChange={(v) => setStreamPartialsMutation.mutate(v ? "true" : "false")}
          >
            Type words as I speak them, instead of waiting for the sentence
          </Check>
        </Group>
      )}


      {on("output.insert", ["undo", "scratch", "retry", "again", "mistake", "wrong", "fix", "take back"]) && (
        <Group
          title={label("output.insert", "When it gets it wrong")}
          hint={
            <>
              <p>
                Both shortcuts are global: by the time you notice, the focus is
                in the app that got the text.
              </p>
              <p className="mt-2">
                Undo sends the focused app its own undo shortcut, so it works
                wherever that does — and can’t delete text you typed yourself
                afterwards.
              </p>
              <p className="mt-2">
                Retry re-runs the audio Echo already has. Nothing leaves this
                machine unless you pick a cloud provider, and the audio is held
                in memory only, one utterance at a time, never written to disk.
              </p>
            </>
          }
        >
          <FixUps />
        </Group>
      )}


      {on("output.insert", ["password", "secure", "safety", "mask", "credential", "login"]) && (
        <Group
          title={label("output.insert", "Password fields")}
          hint={
            secureDetection
              ? "Echo asks the accessibility API whether the focused control is masked. Where it can't tell, it types as normal — refusing whenever the system stays quiet would break dictation in every app that publishes no accessibility tree."
              : "This system can't answer the question, so the guard never fires here. On Linux it would need AT-SPI over D-Bus, and under Wayland usually not even then. Nothing is protecting you — that's why it says so rather than showing a switch that does nothing."
          }
        >
          <Check
            checked={blockSecure !== "false"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "block_secure_fields", value: v ? "true" : "false" })
            }
          >
            Never type into a password field — and never save it to History
          </Check>
          {!secureDetection && (
            <Problem>Not available on this system: the guard can't detect anything here.</Problem>
          )}
        </Group>
      )}


      {/* ---- Output · Formatting ----------------------------------------- */}

      {on("output.formatting", ["punctuation", "comma", "period", "format", "capital", "numbers", "spacing", "tidy"]) && (
        <Group
          title={label("output.formatting", "Formatting")}
          hint="Runs after your dictionary, on the finished sentence. Turn the whole group off for a particular app under Per-app profiles — a terminal usually wants the words exactly as spoken."
        >
          <Check
            checked={autoEdit !== "false"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "auto_edit", value: v ? "true" : "false" })
            }
            hint="Only sounds nobody means to write. Words that are sometimes filler — “like”, “actually”, “basically” — are left alone, because no rule can tell when you meant them. English only."
          >
            Drop “um”, “uh” and stuttered words
          </Check>

          <Check
            checked={autoEditLlm === "true"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "auto_edit_llm", value: v ? "true" : "false" })
            }
            hint="“Send it Tuesday, no, Wednesday” becomes “Send it Wednesday”. Uses the Command mode model on every utterance, so it costs latency — and it is the one setting here that changes the words you said. Off by default for that reason. Your History keeps what you actually said either way."
          >
            Also let the model fix self-corrections
          </Check>

          {/* The catch belongs where you decide, not after you have decided:
              this used to appear only once the setting was already on. Which
              languages have rules is part of it — a speaker of one that doesn't
              would otherwise dictate "coma", get nothing, and reasonably
              conclude the feature is broken. */}
          <Check
            checked={spokenPunctuation === "true"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "spoken_punctuation", value: v ? "true" : "false" })
            }
            hint={
              <>
                <p>
                  The cost of this one is real: “period” and “colon” stop being
                  usable as ordinary words. Echo keeps them when the sentence
                  makes it obvious — “a period of time”, “the colon” — but that
                  is a rule of thumb, not grammar. Off by default for that reason.
                </p>
                <p className="mt-2">
                  Works in{" "}
                  {punctuationLanguages
                    .map((c) => languages.find((l) => l.code === c)?.label ?? c)
                    .join(", ")}
                  . Other languages are left exactly as spoken.
                </p>
              </>
            }
          >
            Let me say punctuation — “comma”, “new paragraph”, “question mark”
          </Check>

          <Check
            checked={formatNumbers !== "false"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "format_numbers", value: v ? "true" : "false" })
            }
            hint="English only: number words are grammar, not a word list."
          >
            Write numbers, times and units as digits — “twenty five” → 25
          </Check>

          <Check
            checked={formatTidy !== "false"}
            onChange={(v) =>
              setFormatSetting.mutate({ key: "format_tidy", value: v ? "true" : "false" })
            }
          >
            Fix spacing around punctuation and capitalise sentences
          </Check>
        </Group>
      )}


      {/* ---- Output · Apps ----------------------------------------------- */}

      {on("output.apps", ["app", "per app", "profile", "profiles", "exclude", "override", "terminal"]) && (
        <Group
          title={label("output.apps", "Per-app profiles")}
          hint="Override how Echo behaves in specific applications."
        >
          <AppProfiles />
        </Group>
      )}


      {/* ---- Output · Advanced ------------------------------------------- */}

      {on("output.advanced", [
        "delay", "timing", "clipboard", "hold", "settle", "slow", "missing",
        "wrong place", "late",
      ]) && (
        <Group
          title={label("output.advanced", "Timing")}
          hint="For when text arrives late, lands somewhere else, or never arrives at all. Leave these alone until it does."
        >
          {pastes && (
            <Field
              label="Clipboard hold (ms)"
              hint="How long Echo leaves the text on your clipboard before restoring what was there. Raise it if text goes missing — Electron apps, terminals and remote desktops often need longer than the default to read it."
            >
              <input
                type="number"
                min={20}
                step={20}
                className="field w-32"
                defaultValue={clipboardSettle ?? "180"}
                onBlur={(e) => setClipboardSettleMutation.mutate(e.target.value || "180")}
              />
            </Field>
          )}

          <Field
            label="Insert delay (ms)"
            hint="A pause before Echo starts typing. Leave it at zero unless text lands in the wrong place — some apps need a moment to take focus back after the pill closes."
          >
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

      {on("output.advanced", ["permission", "accessibility", "xdotool", "ydotool", "macos", "linux"]) && (
        <Group
          title={label("output.advanced", "Permissions")}
          hint={
            <>
              macOS needs Accessibility permission. Linux needs <code>xdotool</code> (X11) or{" "}
              <code>ydotool</code> (Wayland). Windows works out of the box.
            </>
          }
        >
          <div className="flex items-center gap-3">
            <button onClick={checkPermission} className="btn-ghost px-3 py-1.5 text-[13.5px]">
              Check permission
            </button>
            {permissionStatus !== null && (
              // Without colour the icon is what distinguishes these two states.
              <span className="flex items-center gap-1.5 text-[13.5px] font-medium text-[var(--ink)]">
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


      {/* ---- Privacy ------------------------------------------------------ */}

      {on("privacy", ["history", "transcript", "retention", "delete", "export"]) && (
        <Group title={label("privacy", "History")}>
          <Check
            checked={historyEnabled !== "false"}
            onChange={(v) => setHistoryMutation.mutate(v ? "true" : "false")}
          >
            Keep a searchable record of what you dictated
          </Check>

          <Field label="Delete transcripts after">
            <select
              className="field w-full"
              value={retention ?? "0"}
              onChange={(e) => setRetention(e.target.value)}
              disabled={historyEnabled === "false"}
            >
              <option value="0">Never</option>
              <option value="7">7 days</option>
              <option value="30">30 days</option>
              <option value="90">90 days</option>
              <option value="365">1 year</option>
            </select>
          </Field>
        </Group>
      )}


      {on("privacy", ["learn", "auto-learn", "corrections", "dictionary", "teach"]) && (
        <Group
          title={label("privacy", "Learning")}
          hint="When you fix a word in History, Echo can add that correction to your dictionary so the mistake stops happening. Only confident, small corrections are kept, and every one shows up in the Dictionary where you can remove it."
        >
          <Check
            checked={autoLearn !== "false"}
            onChange={(v) => setAutoLearn(v ? "true" : "false")}
          >
            Learn from corrections you make
          </Check>
        </Group>
      )}

      {on("privacy", ["telemetry", "usage", "events", "analytics"]) && (
        <Group title={label("privacy", "Telemetry")}>
          <TelemetrySettings />
        </Group>
      )}


      {on("privacy", ["request", "network", "egress", "offline", "outbound", "privacy"]) && (
        <Group
          title={label("privacy", "Request log")}
          hint="This lists requests Echo itself made. It is not proof that nothing else left your machine — Echo can’t see traffic from other programs, and a native plugin can make requests that bypass this log entirely."
        >
          <EgressLog />
        </Group>
      )}

    </Page>
  );
}
