/**
 * SOURCE OF TRUTH KEYWORDS: Toggle, TextField, NumberField, ChoiceField,
 *   HotkeyField, controls.tsx
 * WHAT:  One input per SettingKind. Each takes a string value and reports a
 *        string back, because that is what the settings table stores.
 * WHY:   Strings all the way through on purpose. The Rust side validates
 *        against the declared kind before a write lands, so the control's job
 *        is to make the legal values easy to pick and the illegal ones hard to
 *        type — not to be the place correctness is decided. A control that
 *        parsed and re-serialised would be a second encoding to keep in step
 *        with the first.
 *
 *        NOTHING HERE CARRIES A HUE. An ON switch, a focused field and a
 *        selected option are all ink — see styles/tokens.css. State is carried
 *        by form: the knob moves, the ring appears, the option fills.
 * WHERE: Switched on by SettingControl.
 */

import clsx from "clsx";

interface FieldProps {
  id: string;
  value: string;
  disabled?: boolean;
  onChange: (next: string) => void;
}

/**
 * The switch. A real checkbox underneath, so it is focusable, announced and
 * togglable by keyboard without any of that being re-implemented.
 */
export function Toggle({ id, value, disabled, onChange }: FieldProps) {
  const on = value === "true";
  return (
    <label
      className={clsx(
        "relative inline-flex shrink-0 cursor-pointer items-center",
        disabled && "cursor-not-allowed opacity-50",
      )}
      style={{ width: 40, height: 24 }}
    >
      <input
        id={id}
        type="checkbox"
        className="peer sr-only"
        checked={on}
        disabled={disabled}
        onChange={(e) => onChange(String(e.target.checked))}
      />
      <span
        aria-hidden
        className="material absolute inset-0 rounded-full transition-colors peer-focus-visible:outline peer-focus-visible:outline-2"
        style={{
          background: on ? "var(--accent)" : "var(--surface-sunken-strong)",
          outlineColor: "var(--accent)",
          outlineOffset: "var(--focus-ring-offset)",
        }}
      />
      <span
        aria-hidden
        className="absolute rounded-full transition-transform"
        style={{
          width: 18,
          height: 18,
          left: 3,
          background: on ? "var(--surface-opaque-elevated)" : "var(--text-secondary)",
          boxShadow: "var(--shadow-card)",
          transform: on ? "translateX(16px)" : "translateX(0)",
          transitionDuration: "var(--motion-duration-fast)",
          transitionTimingFunction: "var(--motion-ease-standard)",
        }}
      />
    </label>
  );
}

export function TextField({
  id,
  value,
  disabled,
  onChange,
  placeholder,
  maxLength,
}: FieldProps & { placeholder?: string; maxLength?: number }) {
  return (
    <input
      id={id}
      type="text"
      className="field focus-ring"
      style={{ maxWidth: 260 }}
      value={value}
      disabled={disabled}
      placeholder={placeholder}
      maxLength={maxLength}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

/**
 * A number, with its unit beside it rather than inside the box — putting "ms"
 * in the placeholder means it disappears the moment there is a value, which is
 * exactly when the reader needs it.
 */
export function NumberField({
  id,
  value,
  disabled,
  onChange,
  min,
  max,
  step,
  unit,
}: FieldProps & { min: number; max: number; step: number; unit?: string | null }) {
  return (
    <span className="inline-flex items-center gap-2">
      <input
        id={id}
        type="number"
        className="field focus-ring tabular"
        style={{ width: 96 }}
        value={value}
        disabled={disabled}
        min={min}
        max={max}
        step={step}
        onChange={(e) => onChange(e.target.value)}
      />
      {unit ? (
        <span style={{ color: "var(--text-tertiary)", fontSize: "var(--text-caption-size)" }}>
          {unit}
        </span>
      ) : null}
    </span>
  );
}

export interface Option {
  value: string;
  label: string;
  description?: string | null;
}

/**
 * Two or three short options render as a segmented control, more as a select.
 *
 * The threshold is about reading, not about taste: a segmented control shows
 * every option at once, which is what makes a three-way choice legible without
 * opening anything — and what makes a twelve-way choice an unreadable strip.
 */
export function ChoiceField({
  id,
  value,
  disabled,
  onChange,
  options,
}: FieldProps & { options: Option[] }) {
  const inline =
    options.length > 1 &&
    options.length <= 3 &&
    options.every((o) => o.label.length <= 24);

  if (!inline) {
    return (
      <select
        id={id}
        className="field focus-ring"
        style={{ maxWidth: 260 }}
        value={value}
        disabled={disabled}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.length === 0 ? <option value="">Nothing available</option> : null}
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    );
  }

  return (
    <div
      id={id}
      role="radiogroup"
      className="material inline-flex"
      style={{
        borderRadius: "var(--segment-radius)",
        padding: "var(--segment-padding)",
        background: "var(--surface-sunken)",
        gap: "var(--segment-padding)",
      }}
    >
      {options.map((o) => {
        const selected = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={selected}
            title={o.description ?? undefined}
            disabled={disabled}
            data-selected={selected}
            onClick={() => onChange(o.value)}
            className="interactive focus-ring"
            style={{
              height: "var(--segment-height)",
              padding: "0 var(--space-3)",
              borderRadius: "calc(var(--segment-radius) - var(--segment-padding))",
              fontSize: "var(--text-label-size)",
              fontWeight: "var(--text-label-weight)",
              // A selected option is an INVERTED FILL, never a hue: it is the
              // only weight available in a monochrome interface, and it is
              // enough.
              background: selected ? "var(--accent)" : "transparent",
              color: selected ? "var(--surface-opaque-elevated)" : "var(--text-secondary)",
              cursor: disabled ? "not-allowed" : "pointer",
              whiteSpace: "nowrap",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * A hotkey is shown, not typed. Capture lives in the existing HotkeyCapture
 * component; this renders the current binding and hands the click on.
 */
export function HotkeyField({
  id,
  value,
  disabled,
  onClick,
}: {
  id: string;
  value: string;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      id={id}
      type="button"
      disabled={disabled}
      onClick={onClick}
      className="material interactive focus-ring"
      style={{
        height: "var(--control-height)",
        padding: "0 var(--space-3)",
        borderRadius: "var(--radius-input)",
        fontFamily: "var(--font-mono-stack)",
        fontSize: "var(--text-mono-size)",
        color: "var(--text-primary)",
        cursor: disabled ? "not-allowed" : "pointer",
      }}
    >
      {value || "Not set"}
    </button>
  );
}
