/**
 * SOURCE OF TRUTH KEYWORDS: SettingControl, SettingControlProps, useDynamicOptions,
 *   dynamic choice, requires_permission
 * WHAT:  Renders ONE setting from its registry declaration: label, explanation,
 *        the right control for its kind, and why it is unavailable when it is.
 * WHY:   This is the component that makes "adding a setting is a registry
 *        entry" true on the frontend. Before it, each setting was a bespoke
 *        block of JSX in a 1361-line panel, which is why the panel had drifted
 *        into three different ways of drawing a toggle.
 *
 *        A setting whose permission is missing renders DISABLED WITH A REASON
 *        rather than hidden or live. Hidden is worse — the user goes looking
 *        for the thing they read about and concludes the app lacks it. Live is
 *        worse still: a toggle that is ON while the permission behind it is
 *        missing is a control that lies.
 * WHERE: Driven by the settings view over the registry snapshot.
 */

import { useQuery } from "@tanstack/react-query";

import { commands } from "../../../ipc/commands";
import { ChoiceField, HotkeyField, NumberField, TextField, Toggle } from "./controls";
import type { Option } from "./controls";
import type { ChoiceSource, OsPermission, SettingDef } from "./types";

/** What each permission is called where the user would go to grant it. */
const PERMISSION_REASON: Record<OsPermission, string> = {
  MICROPHONE: "Echo needs permission to use the microphone before this does anything.",
  ACCESSIBILITY:
    "Echo needs accessibility permission before this does anything. Until then transcripts go to the clipboard.",
};

/**
 * Resolves the options for a runtime-sourced choice.
 *
 * Each source names a query rather than the component knowing how to fetch
 * anything, which is what keeps a device picker a registry entry instead of a
 * bespoke form. An unknown source returns nothing rather than throwing: a
 * settings pane that crashes because one row's source was renamed takes every
 * other row with it.
 */
function useDynamicOptions(source: ChoiceSource | null) {
  return useQuery({
    queryKey: ["setting-options", source],
    enabled: source !== null,
    queryFn: async (): Promise<Option[]> => {
      switch (source) {
        case "INPUT_DEVICES": {
          const devices = await commands.getAudioDevices();
          return [
            { value: "", label: "System default" },
            ...devices.map((d) => ({ value: d.name, label: d.name })),
          ];
        }
        case "WHISPER_MODELS":
        case "NEMO_MODELS": {
          // Only what is actually on disk. Offering a model that still has to
          // be downloaded turns a settings change into a silent failure at the
          // next keypress; downloading is its own flow, with a progress bar.
          const engine = source === "NEMO_MODELS" ? "nemo" : "whisper";
          const models = await commands.listModels();
          return models
            .filter((m) => m.downloaded && m.engine === engine)
            .map((m) => ({
              value: m.name,
              label: m.english_only ? `${m.name} (English only)` : m.name,
            }));
        }
        case "LANGUAGES": {
          const languages = await commands.dictationLanguages();
          return [
            { value: "auto", label: "Detect automatically" },
            ...languages.map((l) => ({ value: l.code, label: l.label })),
          ];
        }
        default:
          return [];
      }
    },
  });
}

/**
 * A setting the machine cannot honour, whatever it is set to.
 *
 * Distinct from a missing permission, which the user can go and grant: this is
 * a fact about the computer — a model that did not load, hardware that is not
 * there. The control shows the value that IS in force rather than the stored
 * one, because showing the stored value would be the control lying in the
 * quietest possible way.
 */
export interface Unavailable {
  /** What is actually in force. */
  value: string;
  /** Said beside the control, in place of a permission reason. */
  reason: string;
}

export interface SettingControlProps {
  def: SettingDef;
  value: string;
  /** Permissions the OS has NOT granted. Empty means everything is available. */
  missingPermissions?: OsPermission[];
  /** Set when this machine cannot honour the setting. See Unavailable. */
  unavailable?: Unavailable;
  onChange: (next: string) => void;
  onCaptureHotkey?: (key: string) => void;
}

export function SettingControl({
  def,
  value: stored,
  missingPermissions = [],
  unavailable,
  onChange,
  onCaptureHotkey,
}: SettingControlProps) {
  const blockedBy = def.requires_permission.find((p) => missingPermissions.includes(p));
  const disabled = blockedBy !== undefined || unavailable !== undefined;
  // What is in force, which is the stored value unless the machine overrides it.
  const value = unavailable?.value ?? stored;
  const id = `setting-${def.key}`;

  const dynamicSource = def.kind.type === "DYNAMIC_CHOICE" ? def.kind.source : null;
  const { data: dynamicOptions } = useDynamicOptions(dynamicSource);

  function control() {
    switch (def.kind.type) {
      case "TOGGLE":
        return <Toggle id={id} value={value} disabled={disabled} onChange={onChange} />;
      case "TEXT":
        return (
          <TextField
            id={id}
            value={value}
            disabled={disabled}
            onChange={onChange}
            placeholder={def.kind.placeholder ?? undefined}
            maxLength={def.kind.max_len ?? undefined}
          />
        );
      case "NUMBER":
        return (
          <NumberField
            id={id}
            value={value}
            disabled={disabled}
            onChange={onChange}
            min={def.kind.min}
            max={def.kind.max}
            step={def.kind.step}
            unit={def.kind.unit}
          />
        );
      case "CHOICE":
        return (
          <ChoiceField
            id={id}
            value={value}
            disabled={disabled}
            onChange={onChange}
            options={def.kind.options}
          />
        );
      case "DYNAMIC_CHOICE":
        return (
          <ChoiceField
            id={id}
            value={value}
            disabled={disabled}
            onChange={onChange}
            options={dynamicOptions ?? []}
          />
        );
      case "HOTKEY":
        return (
          <HotkeyField
            id={id}
            value={value}
            disabled={disabled}
            onClick={() => onCaptureHotkey?.(def.key)}
          />
        );
    }
  }

  return (
    <div className="setting-row">
      <div style={{ minWidth: 0, flex: 1 }}>
        <label
          htmlFor={id}
          style={{
            display: "block",
            color: "var(--text-primary)",
            fontSize: "var(--text-body-size)",
            lineHeight: "var(--text-body-line)",
            fontWeight: "var(--text-label-weight)",
          }}
        >
          {def.label}
        </label>
        <p
          style={{
            margin: "var(--space-1) 0 0",
            color: "var(--text-secondary)",
            fontSize: "var(--text-caption-size)",
            lineHeight: "var(--text-caption-line)",
            maxWidth: "58ch",
          }}
        >
          {def.description}
        </p>
        {unavailable ? (
          <p
            role="alert"
            style={{
              margin: "var(--space-2) 0 0",
              color: "var(--danger)",
              fontSize: "var(--text-caption-size)",
              lineHeight: "var(--text-caption-line)",
              maxWidth: "58ch",
            }}
          >
            {unavailable.reason}
          </p>
        ) : null}
        {blockedBy ? (
          <p
            style={{
              margin: "var(--space-2) 0 0",
              color: "var(--text-tertiary)",
              fontSize: "var(--text-caption-size)",
              lineHeight: "var(--text-caption-line)",
              maxWidth: "58ch",
            }}
          >
            {PERMISSION_REASON[blockedBy]}
          </p>
        ) : null}
      </div>
      <div style={{ paddingTop: 2 }}>{control()}</div>
    </div>
  );
}
