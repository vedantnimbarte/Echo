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
};
