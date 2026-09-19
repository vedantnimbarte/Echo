/**
 * SOURCE OF TRUTH KEYWORDS: SettingDef, SettingKind, SettingValue, Capability,
 *   SettingsSnapshot, ChoiceSource, SettingSection, NavEntry
 * WHAT:  The shapes the registry sends over, as the settings UI sees them.
 * WHY:   Hand-written for now, and deliberately marked as such: these mirror
 *        the specta-generated types in src/lib/bindings.ts, which is written by
 *        a debug run of the app (see src-tauri/src/lib.rs). Once a dev run has
 *        produced that file these should be re-exported from it and this file
 *        deleted — a second copy of a type is exactly what the generated
 *        bindings exist to remove, and leaving one here permanently would undo
 *        the point of generating them.
 * WHERE: Used by SettingControl and the settings view.
 */

export type SettingSection =
  | "SETTINGS_GENERAL"
  | "SETTINGS_DICTATION"
  | "SETTINGS_MICROPHONE"
  | "ENGINE_SPEECH"
  | "ENGINE_TOOLS"
  | "ENGINE_ADVANCED"
  | "OUTPUT_INSERT"
  | "OUTPUT_FORMATTING"
  | "OUTPUT_APPS"
  | "OUTPUT_ADVANCED"
  | "PRIVACY";

export type ChoiceSource =
  | "INPUT_DEVICES"
  | "WHISPER_MODELS"
  | "NEMO_MODELS"
  | "CLOUD_PROVIDERS"
  | "LANGUAGES"
  | "PUNCTUATION_LANGUAGES"
  | "NUMBER_LANGUAGES"
  | "CLEANUP_LANGUAGES"
  | "WAKE_WORD_MODELS"
  | "PROFILES";

export type OsPermission = "MICROPHONE" | "ACCESSIBILITY";

export interface SettingChoice {
  value: string;
  label: string;
  description: string | null;
}

export type SettingKind =
  | { type: "TOGGLE" }
  | { type: "TEXT"; placeholder: string | null; max_len: number | null }
  | {
      type: "NUMBER";
      min: number;
      max: number;
      step: number;
      unit: string | null;
    }
  | { type: "CHOICE"; options: SettingChoice[] }
  | { type: "DYNAMIC_CHOICE"; source: ChoiceSource }
  | { type: "HOTKEY" };

export type SettingValue =
  | { type: "BOOL"; value: boolean }
  | { type: "TEXT"; value: string }
  | { type: "NUMBER"; value: number };

export interface VisibleWhen {
  key: string;
  any_of: string[];
}

export interface SettingDef {
  key: string;
  label: string;
  description: string;
  section: SettingSection;
  kind: SettingKind;
  default: SettingValue;
  requires_permission: OsPermission[];
  advanced: boolean;
  visible_when: VisibleWhen | null;
}

export interface NavDef {
  label: string;
  route: string;
  icon: string;
  order: number;
}

export interface Capability {
  key: string;
  name: string;
  description: string;
  settings: SettingDef[];
  requires: OsPermission[];
  nav: NavDef | null;
  metrics: { stage: string; label: string; user_facing: boolean }[];
  hotkey: { id: string; label: string; default: string; setting_key: string | null } | null;
}

export interface StoredSetting {
  key: string;
  value: string;
  is_set: boolean;
}

export interface SettingsSnapshot {
  capabilities: Capability[];
  values: StoredSetting[];
  nav: { capability: string; nav: NavDef }[];
}

/** The section headings, in the order the settings view renders them. */
export const SECTION_ORDER: SettingSection[] = [
  "SETTINGS_GENERAL",
  "SETTINGS_DICTATION",
  "SETTINGS_MICROPHONE",
  "ENGINE_SPEECH",
  "ENGINE_TOOLS",
  "ENGINE_ADVANCED",
  "OUTPUT_INSERT",
  "OUTPUT_FORMATTING",
  "OUTPUT_APPS",
  "OUTPUT_ADVANCED",
  "PRIVACY",
];

export const SECTION_LABELS: Record<SettingSection, string> = {
  SETTINGS_GENERAL: "General",
  SETTINGS_DICTATION: "Dictation",
  SETTINGS_MICROPHONE: "Microphone",
  ENGINE_SPEECH: "Speech",
  ENGINE_TOOLS: "Tools",
  ENGINE_ADVANCED: "Advanced",
  OUTPUT_INSERT: "Insert",
  OUTPUT_FORMATTING: "Formatting",
  OUTPUT_APPS: "Apps",
  OUTPUT_ADVANCED: "Advanced",
  PRIVACY: "Privacy",
};

/**
 * Which settings page each section belongs to. Mirrors SettingSection::page()
 * in Rust; the two are one fact and move together.
 */
export const SECTION_PAGE: Record<SettingSection, string> = {
  SETTINGS_GENERAL: "settings",
  SETTINGS_DICTATION: "settings",
  SETTINGS_MICROPHONE: "settings",
  ENGINE_SPEECH: "engine",
  ENGINE_TOOLS: "engine",
  ENGINE_ADVANCED: "engine",
  OUTPUT_INSERT: "output",
  OUTPUT_FORMATTING: "output",
  OUTPUT_APPS: "output",
  OUTPUT_ADVANCED: "output",
  PRIVACY: "privacy",
};
