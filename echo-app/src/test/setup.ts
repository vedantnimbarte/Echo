//! Test environment for the frontend: a fake Tauri backend.
//!
//! Every panel in Echo renders by asking the Rust side a question, so nothing
//! mounts in jsdom without `invoke`. The choice is between mocking each command
//! in each test — which makes the tests a transcription of the component under
//! test, and lets a renamed command pass — or one fake backend that answers the
//! way the real one does. This is the second.
//!
//! The fake is deliberately permissive: an unknown command returns `null` rather
//! than throwing, because these are smoke tests and a panel that renders with a
//! missing value is the behaviour we want anyway. What it must not do is make a
//! *wrong* answer look right, so the shapes here match the real return types.

import { vi, afterEach } from "vitest";
import { cleanup } from "@testing-library/react";

/** Settings the fake backend remembers, so set/get round-trips work. */
export const settings = new Map<string, string>();

/** Every command the fake was asked for, in order — assertable in a test. */
export const invoked: string[] = [];

/**
 * Answers keyed by command name. Anything absent falls through to `null`.
 *
 * Only commands the panels actually call on mount need an entry; the rest exist
 * because a shape of `null` would crash a `.map`.
 *
 * Exported and mutable, like `settings`: a test that needs the machine to answer
 * differently — no neural VAD, no GPU — overrides the one key and restores it,
 * rather than re-mocking the whole backend.
 */
export const ANSWERS: Record<string, unknown> = {
  get_audio_devices: [{ name: "Test Microphone", is_default: true }],
  list_models: [
    { name: "base.en", downloaded: true, size_mb: 142, english_only: true },
    { name: "small", downloaded: false, size_mb: 466, english_only: false },
  ],
  list_cloud_providers: [
    {
      id: "openai",
      label: "OpenAI",
      kind: "openai",
      default_endpoint: "https://api.openai.com/v1",
      models: ["whisper-1"],
      needs_endpoint: false,
      needs_region: false,
      docs_url: "https://platform.openai.com/api-keys",
      note: "Whisper on OpenAI's servers.",
      key_set: false,
      model: "whisper-1",
      endpoint: "https://api.openai.com/v1",
      region: null,
      available: true,
    },
  ],
  list_dictionary: [],
  get_history: [],
  list_plugins: [],
  list_profiles: [],
  list_wake_words: [],
  get_egress_log: [],
  // Every one of these returns a Vec on the Rust side, so it is never null.
  // Left out of the fake they came back as one, and the component that mapped
  // over it threw — after `waitFor` had already seen the first paint, so the
  // render tests went green while the panel was dying behind them.
  retry_targets: [],
  list_app_profiles: [],
  installed_packs: [],
  supported_import_formats: ["wav", "mp3", "m4a"],
  /** The directory the scaffold reports having written. */
  scaffold_plugin: "/home/you/plugins/my-plugin",
  get_egress_status: { offline_capable: true, hosts: [] },
  get_telemetry_summary: [],
  get_dictation_stats: null,
  get_insights: {
    transcripts: 3,
    words: 42,
    days: 2,
    words_last_7_days: 42,
    since: "2025-01-02T10:00:00Z",
    spoken_ms: 21_000,
    timed_words: 42,
    timed_transcripts: 3,
    dictionary_fixes: 1,
    cleanup_fixes: 4,
    streak: 2,
    longest_streak: 2,
    apps: [{ key: "code.exe", transcripts: 3, words: 42 }],
    providers: [{ key: "local", transcripts: 3, words: 42 }],
    languages: [{ key: "en", transcripts: 3, words: 42 }],
    hours: Array.from({ length: 24 }, (_, h) => (h === 9 ? 3 : 0)),
    daily: [
      { date: "2025-01-02", words: 20, transcripts: 1 },
      { date: "2025-01-03", words: 22, transcripts: 2 },
    ],
  },
  spoken_punctuation_languages: ["en", "es", "fr", "de", "it", "pt", "nl"],
  number_languages: ["en", "de", "es", "fr", "it", "nl", "pt"],
  diagnostics: "Echo 0.3.0\nOS: windows (x86_64)\nEngine: local\n",
  // Shortened, but `auto` first and real codes, because the language `<select>`
  // ticks against these and the punctuation hint looks labels up in them.
  dictation_languages: [
    { code: "auto", label: "Auto-detect" },
    { code: "en", label: "English" },
    { code: "es", label: "Spanish" },
  ],
  silero_available: true,
  recovered_recordings: [],
  get_hotkey: "CommandOrControl+Shift+Space",
  secure_field_detection: true,
  wake_word_ready: false,
  wake_word_active: false,
  wake_word_status: "disabled",
  is_recording: false,
  get_autostart: false,
  check_accessibility_permission: true,
  gpu_status: {
    detected: "CPU only",
    available_pack: null,
    available_pack_mb: null,
    pack_installed: false,
    active: false,
    failed: false,
    enabled: false,
    threads: 4,
  },
  hotkey_support: {
    session: "native",
    desktop: null,
    can_bind: true,
    supports_bare_modifier: true,
    advice: "",
  },
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string, args?: Record<string, unknown>) => {
    invoked.push(command);

    if (command === "get_setting") return settings.get(args?.key as string) ?? null;
    if (command === "set_setting") {
      settings.set(args?.key as string, args?.value as string);
      return null;
    }
    // Registers the provider *and* persists it, the way the real command does,
    // so a test can read back which engine a click actually chose.
    if (command === "set_asr_provider") {
      settings.set("asr_provider", args?.name as string);
      return null;
    }
    return command in ANSWERS ? ANSWERS[command] : null;
  }),
}));

// Event subscriptions return their own unsubscribe function, which the hooks
// call on unmount — returning undefined here would fail the cleanup, not the
// component.
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

const fakeWindow = {
  show: vi.fn(async () => {}),
  hide: vi.fn(async () => {}),
  setFocus: vi.fn(async () => {}),
  close: vi.fn(async () => {}),
  setSize: vi.fn(async () => {}),
  startDragging: vi.fn(async () => {}),
  minimize: vi.fn(async () => {}),
  toggleMaximize: vi.fn(async () => {}),
  isMaximized: vi.fn(async () => false),
  onCloseRequested: vi.fn(async () => () => {}),
  onResized: vi.fn(async () => () => {}),
  label: "main",
};

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => fakeWindow,
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: { getByLabel: vi.fn(async () => fakeWindow) },
  getCurrentWebviewWindow: () => fakeWindow,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
  message: vi.fn(async () => {}),
  confirm: vi.fn(async () => false),
}));

// Every external link leaves the webview through this, so a test that clicked
// one would otherwise try to launch a browser on the machine running it.
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {}),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(async () => "0.3.0"),
}));

vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: vi.fn(async () => {}),
  exit: vi.fn(async () => {}),
}));

// No update check during tests: it would be the one mock that reaches out.
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(async () => null),
}));

afterEach(() => {
  cleanup();
  settings.clear();
  invoked.length = 0;
});
