import { invoke } from "@tauri-apps/api/core";

export interface AudioDevice {
  name: string;
  is_default: boolean;
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
  profile_id: number | null;
  enabled: boolean;
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

  quit: () => invoke<void>("quit"),

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

  registerHotkey: (shortcut: string) =>
    invoke<void>("register_hotkey", { shortcut }),
};
