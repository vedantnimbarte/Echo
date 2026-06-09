import { useState } from "react";
import clsx from "clsx";

/**
 * Click-to-record keyboard-chord field. Replaces hand-typing accelerator
 * strings like "CommandOrControl+Shift+Space": the user clicks, presses the
 * combination, and we emit it in Tauri's accelerator format via `onChange`.
 */
export function HotkeyCapture({
  value,
  onChange,
}: {
  value: string;
  onChange: (accelerator: string) => void;
}) {
  const [recording, setRecording] = useState(false);

  function onKeyDown(e: React.KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      setRecording(false);
      return;
    }

    const accel = toAccelerator(e);
    if (accel) {
      onChange(accel);
      setRecording(false);
    }
  }

  return (
    <button
      type="button"
      onClick={() => setRecording(true)}
      onBlur={() => setRecording(false)}
      onKeyDown={recording ? onKeyDown : undefined}
      className={clsx(
        "flex min-h-[34px] w-full items-center justify-center gap-1 rounded-lg border px-2.5 py-1.5 text-[13px] outline-none transition",
        recording
          ? "border-[var(--aurora-2)]/70 bg-[var(--aurora-2)]/10 text-[var(--ink)]"
          : "border-white/10 bg-white/5 text-[var(--ink)] hover:bg-white/8"
      )}
    >
      {recording ? (
        <span className="text-[12px] text-[var(--ink-muted)]">
          Press a key combination…
        </span>
      ) : value ? (
        prettyHotkey(value).map((k) => (
          <kbd
            key={k}
            className="rounded border border-white/12 bg-white/8 px-1.5 py-px text-[11px] font-medium text-[var(--ink)]"
          >
            {k}
          </kbd>
        ))
      ) : (
        <span className="text-[12px] text-[var(--ink-faint)]">
          Click to set a shortcut
        </span>
      )}
    </button>
  );
}

/** Convert a keydown event to a Tauri accelerator, or null if only modifiers. */
function toAccelerator(e: React.KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.ctrlKey || e.metaKey) mods.push("CommandOrControl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");

  const key = mainKey(e);
  if (!key) return null; // modifier-only — keep waiting
  return [...mods, key].join("+");
}

/** Map a KeyboardEvent's physical key to a Tauri key name (null for modifiers). */
function mainKey(e: React.KeyboardEvent): string | null {
  const code = e.code;
  if (/^(Control|Shift|Alt|Meta)(Left|Right)?$/.test(code)) return null;

  if (code.startsWith("Key")) return code.slice(3); // KeyA → A
  if (code.startsWith("Digit")) return code.slice(5); // Digit1 → 1
  if (code.startsWith("Numpad")) return code; // leave as-is
  if (/^F\d{1,2}$/.test(code)) return code; // F1..F12

  const named: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    Comma: ",",
    Period: ".",
    Slash: "/",
    Backquote: "`",
    Minus: "-",
    Equal: "=",
  };
  return named[code] ?? null;
}

/** Format a stored accelerator for display, e.g. ["Ctrl","Shift","Space"]. */
export function prettyHotkey(raw: string): string[] {
  return raw
    .replace("CommandOrControl", "Ctrl")
    .replace("Control", "Ctrl")
    .replace("Super", "Win")
    .split("+");
}
