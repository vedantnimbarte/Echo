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
}

export interface TelemetrySummaryItem {
  event_type: string;
  count: number;
}

export interface PluginInfo {
  name: string;
  version: string;
  description: string;
  author: string;
  enabled: boolean;
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

  getSetting: (key: string) => invoke<string | null>("get_setting", { key }),

  setSetting: (key: string, value: string) =>
    invoke<void>("set_setting", { key, value }),

  listModels: () => invoke<ModelInfo[]>("list_models"),

  downloadModel: (name: string) => invoke<void>("download_model", { name }),

  setAsrProvider: (name: string) => invoke<void>("set_asr_provider", { name }),

  checkAccessibilityPermission: () =>
    invoke<boolean>("check_accessibility_permission"),

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

  installPlugin: (path: string) => invoke<void>("install_plugin", { path }),

  enablePlugin: (name: string) => invoke<void>("enable_plugin", { name }),

  disablePlugin: (name: string) => invoke<void>("disable_plugin", { name }),

  uninstallPlugin: (name: string) =>
    invoke<void>("uninstall_plugin", { name }),
};
