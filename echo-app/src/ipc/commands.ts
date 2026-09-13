import { invoke } from "@tauri-apps/api/core";

export interface AudioDevice {
  name: string;
  is_default: boolean;
}

export interface Language {
  code: string;
  label: string;
}

export interface DictionaryEntry {
  id: number | null;
  phrase: string;
  replacement: string;
  enabled: boolean;
  profile_id: number | null;
  created_at: string;
}

export interface TranscriptionRecord {
  id: number | null;
  text: string;
  language: string | null;
  provider: string;
  created_at: string;
}

/** The non-secret fields a cloud provider can carry beside its API key. */
export type ProviderField = "model" | "endpoint" | "region";

/**
 * One row of the backend's provider catalog (`core/asr/catalog.rs`), which is
 * the single source of truth for which providers exist. The UI renders whatever
 * this returns rather than keeping its own list — that duplication is exactly
 * what let providers half-exist before.
 */
export interface CloudProvider {
  id: string;
  label: string;
  kind: string;
  default_endpoint: string;
  /** Suggested models; may be empty when the provider takes free text. */
  models: string[];
  needs_endpoint: boolean;
  needs_region: boolean;
  docs_url: string;
  /** Latency, limits, and other things people otherwise learn the hard way. */
  note: string;
  key_set: boolean;
  model: string;
  endpoint: string;
  region: string | null;
  /** False while a catalog row exists but its provider isn't built yet. */
  available: boolean;
}

export interface GpuStatus {
  /** Human-readable detected backend, e.g. "NVIDIA CUDA 12.x". */
  detected: string;
  /** Id of the accelerated pack this machine could run, if any. */
  available_pack: string | null;
  /** Its download size in MB, so the user knows what they are agreeing to. */
  available_pack_mb: number | null;
  pack_installed: boolean;
  /** Whether acceleration is actually in use for the next utterance. */
  active: boolean;
  /** True once an accelerated run failed and Echo latched to CPU. */
  failed: boolean;
  enabled: boolean;
  threads: number;
}

export interface HotkeySupport {
  session: "native" | "x11" | "wayland" | "unknown";
  desktop: string | null;
  /** Whether Echo can register the hotkey itself. */
  can_bind: boolean;
  supports_bare_modifier: boolean;
  /** Plain-language explanation; empty when everything works. */
  advice: string;
}

export interface ModelInfo {
  name: string;
  downloaded: boolean;
  size_mb: number;
  english_only: boolean;
}

export interface TelemetrySummaryItem {
  event_type: string;
  count: number;
}

export interface WakePhraseInfo {
  id: string;
  label: string;
  downloaded: boolean;
  custom: boolean;
}

export interface PluginInfo {
  name: string;
  version: string;
  description: string;
  author: string;
  enabled: boolean;
  /** What the plugin declares it needs. Advisory — not enforced. */
  permissions: string[];
}

export interface PluginManifest {
  name: string;
  version: string;
  description: string;
  author: string;
  permissions: string[];
  entry: string;
}

/** How the last dictionary sync through a shared folder went. */
export interface DictionarySyncStatus {
  /** RFC 3339. When a sync last succeeded; kept when a later one fails. */
  last_synced_at: string | null;
  last_error: string | null;
  /** Conflicted copies the sync service made, merged in on the last sync. */
  conflict_copies: string[];
}

/** A dictionary profile: a named group of entries. */
export interface Profile {
  id: number | null;
  name: string;
  created_at: string;
  updated_at: string;
}

/**
 * Per-app overrides. A null override field means "inherit the global setting",
 * so a profile can pin one behaviour without freezing the rest.
 */
export interface AppProfile {
  id: number | null;
  /** Lowercased executable name, bundle id, or window class. */
  app_match: string;
  label: string | null;
  auto_inject: boolean | null;
  injection_method: string | null;
  /** Type partials as you speak. Null inherits the global setting. */
  stream_partials: boolean | null;
  /** Run the formatting pass here. Null inherits the global setting. */
  formatting: boolean | null;
  profile_id: number | null;
  enabled: boolean;
}

/** What dictation has added up to. Derived from History, so empty when it is off. */
export interface DictationStats {
  transcripts: number;
  words: number;
  /** Distinct days with at least one transcript. */
  days: number;
  words_last_7_days: number;
  /** Earliest transcript still stored; retention trims old rows. */
  since: string | null;
}

/** One row of a "how much of it was X" breakdown: an app, a provider, a language. */
export interface Tally {
  key: string;
  transcripts: number;
  words: number;
}

/** A day that had dictation in it. Days with none are simply absent. */
export interface DayWords {
  /** ISO `YYYY-MM-DD`. */
  date: string;
  words: number;
  transcripts: number;
}

/** Everything the Insights page shows. Derived from History, like the stats above. */
export interface Insights {
  transcripts: number;
  words: number;
  days: number;
  words_last_7_days: number;
  since: string | null;
  /** Speech time and the words spoken in it — the two halves of words-per-minute. */
  spoken_ms: number;
  timed_words: number;
  timed_transcripts: number;
  dictionary_fixes: number;
  cleanup_fixes: number;
  streak: number;
  longest_streak: number;
  apps: Tally[];
  providers: Tally[];
  languages: Tally[];
  /** Transcripts per hour of the day, 24 entries starting at midnight. */
  hours: number[];
  /** The last 365 days that had dictation, oldest first. */
  daily: DayWords[];
}

export interface EgressRecord {
  id: number | null;
  host: string;
  purpose: string;
  created_at: string;
}

export interface EgressStatus {
  offline_capable: boolean;
  reasons: string[];
  recent_count: number;
}

export const commands = {
  getAudioDevices: () => invoke<AudioDevice[]>("get_audio_devices"),

  startRecording: (deviceName?: string, language?: string) =>
    invoke<void>("start_recording", { deviceName, language }),

  stopRecording: () => invoke<void>("stop_recording"),

  isRecording: () => invoke<boolean>("is_recording"),

  listDictionary: () => invoke<DictionaryEntry[]>("list_dictionary"),

  addDictionaryEntry: (phrase: string, replacement: string) =>
    invoke<number>("add_dictionary_entry", { phrase, replacement }),

  deleteDictionaryEntry: (id: number) =>
    invoke<void>("delete_dictionary_entry", { id }),

  toggleDictionaryEntry: (id: number, enabled: boolean) =>
    invoke<void>("toggle_dictionary_entry", { id, enabled }),

  exportDictionary: (path: string) =>
    invoke<void>("export_dictionary", { path }),

  importDictionary: (path: string) =>
    invoke<number>("import_dictionary", { path }),

  /** Sync with the folder now, if sync is on. Resolves to how it went. */
  syncDictionaryNow: () => invoke<DictionarySyncStatus>("sync_dictionary_now"),

  getDictionarySyncStatus: () =>
    invoke<DictionarySyncStatus>("get_dictionary_sync_status"),

  getHistory: (limit?: number) =>
    invoke<TranscriptionRecord[]>("get_history", { limit }),

  clearHistory: () => invoke<void>("clear_history"),

  exportHistory: (path: string) => invoke<void>("export_history", { path }),

  getForegroundApp: () => invoke<string | null>("get_foreground_app"),

  listAppProfiles: () => invoke<AppProfile[]>("list_app_profiles"),

  saveAppProfile: (profile: AppProfile) =>
    invoke<number>("save_app_profile", { profile }),

  deleteAppProfile: (id: number) => invoke<void>("delete_app_profile", { id }),

  listProfiles: () => invoke<Profile[]>("list_profiles"),

  addProfile: (name: string) => invoke<number>("add_profile", { name }),

  deleteProfile: (id: number) => invoke<void>("delete_profile", { id }),

  setDictionaryEntryProfile: (id: number, profileId: number | null) =>
    invoke<void>("set_dictionary_entry_profile", { id, profileId }),

  getEgressLog: (limit?: number) =>
    invoke<EgressRecord[]>("get_egress_log", { limit }),

  clearEgressLog: () => invoke<void>("clear_egress_log"),

  getEgressStatus: () => invoke<EgressStatus>("get_egress_status"),

  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),

  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),

  listModels: () => invoke<ModelInfo[]>("list_models"),

  downloadModel: (name: string) => invoke<void>("download_model", { name }),
  deleteModel: (name: string) => invoke<void>("delete_model", { name }),

  setAsrProvider: (name: string) => invoke<void>("set_asr_provider", { name }),

  setWhisperModel: (name: string) =>
    invoke<void>("set_whisper_model", { name }),

  whisperReady: () => invoke<boolean>("whisper_ready"),

  downloadWhisperBinary: () => invoke<void>("download_whisper_binary"),

  checkAccessibilityPermission: () =>
    invoke<boolean>("check_accessibility_permission"),

  injectText: (text: string) => invoke<void>("inject_text", { text }),

  setApiKey: (provider: string, key: string) =>
    invoke<void>("set_api_key", { provider, key }),

  getApiKeySet: (provider: string) =>
    invoke<boolean>("get_api_key_set", { provider }),

  removeApiKey: (provider: string) =>
    invoke<void>("remove_api_key", { provider }),

  listCloudProviders: () =>
    invoke<CloudProvider[]>("list_cloud_providers"),

  setProviderSetting: (provider: string, field: ProviderField, value: string) =>
    invoke<void>("set_provider_setting", { provider, field, value }),

  testApiKey: (provider: string) =>
    invoke<string>("test_api_key", { provider }),

  getTelemetrySummary: () =>
    invoke<TelemetrySummaryItem[]>("get_telemetry_summary"),

  clearTelemetry: () => invoke<void>("clear_telemetry"),

  setTelemetryEnabled: (enabled: boolean) =>
    invoke<void>("set_telemetry_enabled", { enabled }),

  recordTelemetryEvent: (eventType: string, payload?: unknown) =>
    invoke<void>("record_telemetry_event", { eventType, payload }),

  listPlugins: () => invoke<PluginInfo[]>("list_plugins"),

  /** Read a plugin's manifest without installing it. */
  inspectPlugin: (path: string) =>
    invoke<PluginManifest>("inspect_plugin", { path }),

  /**
   * `acknowledged` must be true and the caller must have shown the user what a
   * plugin can do — the backend refuses otherwise.
   */
  installPlugin: (path: string, acknowledged: boolean) =>
    invoke<void>("install_plugin", { path, acknowledged }),

  enablePlugin: (name: string) => invoke<void>("enable_plugin", { name }),

  disablePlugin: (name: string) => invoke<void>("disable_plugin", { name }),

  uninstallPlugin: (name: string) =>
    invoke<void>("uninstall_plugin", { name }),

  /**
   * Write a starter plugin project into `parentDir/<name>/`, returning the
   * directory it created. Refuses a name that is not a plain crate name, and
   * refuses to overwrite a directory that already exists.
   */
  scaffoldPlugin: (parentDir: string, name: string) =>
    invoke<string>("scaffold_plugin", { parentDir, name }),

  quit: () => invoke<void>("quit"),

  /** Read from the OS each time, not from echo.db — the registration is not ours. */
  getAutostart: () => invoke<boolean>("get_autostart"),

  /** The logged-in account's given name, or null when the OS offers nothing usable. */
  accountName: () => invoke<string | null>("account_name"),

  setAutostart: (enabled: boolean) =>
    invoke<void>("set_autostart", { enabled }),

  listWakeWords: () => invoke<WakePhraseInfo[]>("list_wake_words"),

  downloadWakeModel: (name: string) =>
    invoke<void>("download_wake_model", { name }),

  importWakeModel: (path: string) => invoke<void>("import_wake_model", { path }),

  setWakeWordEnabled: (enabled: boolean) =>
    invoke<void>("set_wake_word_enabled", { enabled }),

  setWakeWordModel: (name: string) =>
    invoke<void>("set_wake_word_model", { name }),

  setWakeWordSensitivity: (threshold: number) =>
    invoke<void>("set_wake_word_sensitivity", { threshold }),

  wakeWordReady: () => invoke<boolean>("wake_word_ready"),

  wakeWordActive: () => invoke<boolean>("wake_word_active"),

  /** "disabled" | "model-missing" | "listening" | "idle" */
  wakeWordStatus: () => invoke<string>("wake_word_status"),

  getHotkey: () => invoke<string>("get_hotkey"),

  // Persisted through its own command, not setSetting: hold-to-talk needs the
  // key's release as well as its press, so the hotkey is rebound to match.
  setRecordingMode: (mode: string) =>
    invoke<void>("set_recording_mode", { mode }),
  gpuStatus: () => invoke<GpuStatus>("gpu_status"),

  /** Download the accelerated whisper build this machine can run. */
  downloadGpuPack: () => invoke<void>("download_gpu_pack"),

  /** Also clears a latched failure, so it doubles as "try the GPU again". */
  setGpuEnabled: (enabled: boolean) =>
    invoke<void>("set_gpu_enabled", { enabled }),

  /** A number as a string, or "auto". */
  setWhisperThreads: (threads: string) =>
    invoke<void>("set_whisper_threads", { threads }),

  installedPacks: () => invoke<string[]>("installed_packs"),

  /** Open the microphone before it is needed, so recording starts instantly. */
  warmMicrophone: () => invoke<void>("warm_microphone"),

  /**
   * Learn dictionary entries from a transcript the user edited by hand.
   * Returns only what was actually stored — usually nothing.
   */
  learnFromCorrection: (original: string, edited: string) =>
    invoke<{ phrase: string; replacement: string; enabled: boolean }[]>(
      "learn_from_correction",
      { original, edited },
    ),

  transcribeFile: (path: string, language?: string) =>
    invoke<string>("transcribe_file", { path, language }),

  supportedImportFormats: () => invoke<string[]>("supported_import_formats"),

  hotkeySupport: () => invoke<HotkeySupport>("hotkey_support"),

  registerHotkey: (shortcut: string) =>
    invoke<void>("register_hotkey", { shortcut }),

  /* ---- fixing a transcript after it was typed --------------------------- */

  /** Take back the last insert. False means there was nothing to undo. */
  undoLastInsert: () => invoke<boolean>("undo_last_insert"),

  /**
   * Re-decode the last utterance on the configured retry target and replace
   * what was typed. Rejects when there is no recent dictation to retry.
   */
  retryLast: () => invoke<string | null>("retry_last"),

  /** Downloaded local models plus registered cloud providers. */
  retryTargets: () => invoke<string[]>("retry_targets"),

  /** `[undo, retry]` accelerators, or the literal "off". */
  getFixupHotkeys: () => invoke<[string, string]>("get_fixup_hotkeys"),

  /** `which` is "undo" or "retry"; pass "off" to unbind. */
  setFixupHotkey: (which: "undo" | "retry", shortcut: string) =>
    invoke<void>("set_fixup_hotkey", { which, shortcut }),

  /**
   * Whether this machine can tell a password field from an ordinary one.
   * "partial" is Linux with session accessibility off, where only GTK apps
   * answer; "unavailable" means no protection is in force at all.
   */
  secureFieldDetection: () =>
    invoke<"available" | "partial" | "unavailable">("secure_field_detection"),

  /**
   * Language codes that spoken punctuation has rules for. Anything else is
   * left alone rather than being given the English words.
   */
  spokenPunctuationLanguages: () =>
    invoke<string[]>("spoken_punctuation_languages"),

  /** Language codes that number conversion has a parser for. */
  numberLanguages: () => invoke<string[]>("number_languages"),

  /** Language codes that filler and stutter cleanup has rules for. */
  cleanupLanguages: () => invoke<string[]>("cleanup_languages"),

  /**
   * The dictation languages Echo offers. Lives in Rust because the tray menu
   * renders the same list, and two copies drift.
   */
  dictationLanguages: () => invoke<Language[]>("dictation_languages"),

  /**
   * The version, platform, engine and hotkey state a bug report needs, as the
   * block Echo pastes into one. Shown to the user before it goes anywhere.
   */
  diagnostics: () => invoke<string>("diagnostics"),

  /** Show `echo.log` in the system file manager. */
  openLog: () => invoke<void>("open_log"),

  /**
   * Audio rescued from a session that ended without finishing. Absolute paths,
   * newest first; empty is the normal answer.
   */
  recoveredRecordings: () => invoke<string[]>("recovered_recordings"),

  /** Delete one recovered recording once the user is done with it. */
  discardRecovered: (path: string) => invoke<void>("discard_recovered", { path }),

  /**
   * Whether the neural voice-activity model loaded. False means the energy
   * detector is running whatever the `vad_engine` setting says.
   */
  sileroAvailable: () => invoke<boolean>("silero_available"),

  getDictationStats: () => invoke<DictationStats>("get_dictation_stats"),

  getInsights: () => invoke<Insights>("get_insights"),
};
