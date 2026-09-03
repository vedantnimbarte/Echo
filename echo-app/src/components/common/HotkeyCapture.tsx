import { useRef, useState } from "react";
import clsx from "clsx";

/**
 * Click-to-record shortcut field.
 *
 * Handles three shapes: a combination (Ctrl+Shift+Space), a single key that
 * types nothing (F9, Insert), and a modifier on its own (Ctrl). The last one
 * can only be told apart from the start of a combination by waiting for the
 * key to come back up with nothing pressed in between — the same rule the
 * backend applies when watching for it.
 *
 * Keys that type something are refused when bound alone: a global hotkey
 * swallows its key, so binding `/` means `/` stops reaching every other app on
 * the machine for as long as Echo runs.
 */

/** Single keys that are safe to bind alone, because they type nothing. */
const SAFE_ALONE = /^(F\d{1,2}|Insert|Pause|ScrollLock)$/;

const MODIFIER_NAME: Record<string, string> = {
  ControlLeft: "Control",
  ControlRight: "Control",
  AltLeft: "Alt",
  AltRight: "Alt",
  ShiftLeft: "Shift",
  ShiftRight: "Shift",
  MetaLeft: "Meta",
  MetaRight: "Meta",
};

export function HotkeyCapture({
  value,
  onChange,
}: {
  value: string;
  onChange: (accelerator: string) => void;
}) {
  const [recording, setRecording] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  /** Modifiers pressed during this capture; a bare binding needs exactly one. */
  const mods = useRef<Set<string>>(new Set());
  /** A non-modifier was pressed, so nothing here is a bare modifier any more. */
  const chorded = useRef(false);

  function begin() {
    mods.current = new Set();
    chorded.current = false;
    setProblem(null);
    setRecording(true);
  }

  function commit(accelerator: string) {
    onChange(accelerator);
    setRecording(false);
  }

  function onKeyDown(e: React.KeyboardEvent) {
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      setRecording(false);
      return;
    }

    const modifier = MODIFIER_NAME[e.code];
    if (modifier) {
      // Might be a binding on its own, might be the start of a combination.
      // Only the key coming back up settles it.
      mods.current.add(modifier);
      return;
    }

    chorded.current = true;
    const key = mainKey(e);
    if (!key) return; // a key we have no name for — keep waiting

    const held = heldModifiers(e);
    if (held.length === 0 && !SAFE_ALONE.test(key)) {
      setProblem(
        `${key} would stop reaching other apps while Echo runs. Add a modifier, or pick a function key.`
      );
      return;
    }
    commit([...held, key].join("+"));
  }

  function onKeyUp(e: React.KeyboardEvent) {
    if (!recording) return;
    const modifier = MODIFIER_NAME[e.code];
    if (!modifier || chorded.current) return;
    // Still holding another modifier: this is a combination being assembled.
    if (e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
    if (mods.current.size !== 1) return;
    commit(modifier);
  }

  return (
    <div className="space-y-1.5">
      <button
        type="button"
        onClick={begin}
        onBlur={() => setRecording(false)}
        onKeyDown={recording ? onKeyDown : undefined}
        onKeyUp={recording ? onKeyUp : undefined}
        className={clsx(
          "flex min-h-[34px] w-full items-center justify-center gap-1 rounded-lg border px-2.5 py-1.5 text-[13px] outline-none transition",
          recording
            ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] text-[var(--ink)]"
            : "border-[var(--hairline)] bg-[var(--surface-1)] text-[var(--ink)] hover:bg-[var(--surface-2)]"
        )}
      >
        {recording ? (
          <span className="text-[12px] text-[var(--ink-muted)]">
            Press a key or combination…
          </span>
        ) : value ? (
          prettyHotkey(value).map((k, i) => (
            <kbd
              key={`${k}-${i}`}
              className="rounded border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-px text-[11px] font-medium text-[var(--ink)]"
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

      {problem && (
        <p className="text-[10.5px] leading-relaxed text-[var(--ink-faint)]">{problem}</p>
      )}
    </div>
  );
}

/** Modifier names currently held, in a stable order. */
function heldModifiers(e: React.KeyboardEvent): string[] {
  const held: string[] = [];
  if (e.ctrlKey || e.metaKey) held.push("CommandOrControl");
  if (e.altKey) held.push("Alt");
  if (e.shiftKey) held.push("Shift");
  return held;
}

/** Map a physical key to a Tauri key name (null for modifiers and unknowns). */
function mainKey(e: React.KeyboardEvent): string | null {
  const code = e.code;
  if (MODIFIER_NAME[code]) return null;

  if (code.startsWith("Key")) return code.slice(3); // KeyA → A
  if (code.startsWith("Digit")) return code.slice(5); // Digit1 → 1
  if (code.startsWith("Numpad")) return code; // leave as-is
  if (/^F\d{1,2}$/.test(code)) return code; // F1..F24

  const named: Record<string, string> = {
    Space: "Space",
    Enter: "Enter",
    Tab: "Tab",
    Backspace: "Backspace",
    Insert: "Insert",
    Delete: "Delete",
    Home: "Home",
    End: "End",
    PageUp: "PageUp",
    PageDown: "PageDown",
    Pause: "Pause",
    ScrollLock: "ScrollLock",
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
  const label: Record<string, string> = {
    CommandOrControl: "Ctrl",
    CmdOrCtrl: "Ctrl",
    Control: "Ctrl",
    Meta: "Win",
    Super: "Win",
    Command: "Cmd",
  };
  return raw.split("+").map((part) => label[part] ?? part);
}
