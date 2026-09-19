/**
 * SOURCE OF TRUTH KEYWORDS: LaunchAtLogin, MicrophoneTest, GlobalHotkey,
 *   PillSizePicker, EXTRAS
 * WHAT:  The settings controls that cannot be a registry declaration, each with
 *        the reason it cannot.
 * WHY:   The registry generates a control from a VALUE: a key in the settings
 *        table, a kind, a default. Everything here fails that test for a
 *        specific reason, and the reason is what keeps this file from becoming
 *        the 1361-line panel again:
 *
 *          LaunchAtLogin  — the state lives with the OS (a registry Run key, a
 *                           LaunchAgent, an XDG .desktop file), not in Echo's
 *                           settings table. Mirroring it into a row would give
 *                           two sources that can disagree, and the one the user
 *                           sees would be the wrong one.
 *          MicrophoneTest — an action, not a value. Nothing is stored.
 *          GlobalHotkey   — the value IS a registry setting, but capturing one
 *                           means reading raw key events and registering the
 *                           binding with the OS, which can fail in ways the
 *                           user has to be told about. The row is generated;
 *                           the capture is this.
 *          PillSizePicker — the value is a registry setting and renders fine as
 *                           a plain choice, but the three options differ in a
 *                           way words do not carry, and the pill is a separate
 *                           webview that has to be TOLD, not just persisted.
 *
 *        If something new wants to live here, check it fails one of those tests
 *        first. "It needs a nicer layout" is not one of them.
 * WHERE: Composed into SettingsView's EXTRAS table.
 */

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { commands, type InputTest } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { errorMessage } from "../../lib/errors";
import { HotkeyCapture } from "../common/HotkeyCapture";
import type { PillSize } from "../pill/Pill";

/** Inline problem report, in the one place the failing control lives. */
function Problem({ children }: { children: React.ReactNode }) {
  return (
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
      {children}
    </p>
  );
}

function Explain({ children }: { children: React.ReactNode }) {
  return (
    <p
      style={{
        margin: "var(--space-1) 0 var(--space-3)",
        color: "var(--text-secondary)",
        fontSize: "var(--text-caption-size)",
        lineHeight: "var(--text-caption-line)",
        maxWidth: "58ch",
      }}
    >
      {children}
    </p>
  );
}

/**
 * Echo lives in the tray, and a hotkey can only answer if Echo is already
 * running — which is what makes starting at login the difference between part
 * of the keyboard and an app you remember to open.
 */
export function LaunchAtLogin() {
  const qc = useQueryClient();
  const { data: enabled = false } = useQuery({
    queryKey: ["autostart"],
    queryFn: commands.getAutostart,
  });
  const set = useMutation({
    mutationFn: commands.setAutostart,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["autostart"] }),
  });

  return (
    <div className="setting-row">
      <div style={{ minWidth: 0, flex: 1 }}>
        <label
          htmlFor="autostart"
          style={{
            display: "block",
            color: "var(--text-primary)",
            fontSize: "var(--text-body-size)",
            fontWeight: "var(--text-label-weight)",
          }}
        >
          Start Echo when I log in
        </label>
        <Explain>
          Echo lives in the tray — the notification area on Windows, the menu bar on macOS. A
          hotkey can only answer if Echo is already running.
        </Explain>
        {set.isError ? (
          <Problem>
            {errorMessage(set.error)} — on a managed or locked-down machine this is set by
            whoever administers it.
          </Problem>
        ) : null}
      </div>
      <input
        id="autostart"
        type="checkbox"
        className="focus-ring"
        checked={enabled}
        onChange={(e) => set.mutate(e.target.checked)}
        style={{ width: 18, height: 18, accentColor: "var(--accent)", marginTop: 4 }}
      />
    </div>
  );
}

/**
 * Records a moment of audio and reports how loud it actually was.
 *
 * The verdict is the point: "your microphone works" is not something a device
 * list can tell you, and a level meter you have to interpret yourself is how
 * people conclude the app is broken when the input is simply muted.
 */
export function MicrophoneTest() {
  const [result, setResult] = useState<InputTest | null>(null);
  const [running, setRunning] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  const { data: device } = useQuery({
    queryKey: ["setting", "audio_device"],
    queryFn: () => commands.getSetting("audio_device"),
  });

  async function run() {
    setRunning(true);
    setResult(null);
    setFailed(null);
    try {
      setResult(await commands.testInputLevel(device || undefined));
    } catch (e) {
      setFailed(errorMessage(e));
    } finally {
      setRunning(false);
    }
  }

  return (
    <div style={{ paddingTop: "var(--space-2)" }}>
      <button
        type="button"
        onClick={run}
        disabled={running}
        className="material interactive focus-ring"
        style={{
          height: "var(--control-height)",
          padding: "0 var(--space-4)",
          borderRadius: "var(--radius-input)",
          color: "var(--text-primary)",
          fontSize: "var(--text-label-size)",
          fontWeight: "var(--text-label-weight)",
          cursor: running ? "progress" : "pointer",
        }}
      >
        {running ? "Listening…" : "Check the microphone"}
      </button>

      {result ? (
        <p
          style={{
            margin: "var(--space-3) 0 0",
            color:
              // The one place a verdict earns colour, and only for the failing
              // one: silence is the case where saying nothing reads as working.
              result.verdict === "silent" ? "var(--danger)" : "var(--text-secondary)",
            fontSize: "var(--text-caption-size)",
            lineHeight: "var(--text-caption-line)",
            maxWidth: "58ch",
          }}
        >
          <span className="tabular">{result.peak_dbfs.toFixed(0)} dBFS</span> — {result.advice}
        </p>
      ) : null}

      {failed ? <Problem>{failed}</Problem> : null}
    </div>
  );
}

/**
 * Capturing the dictation hotkey, and saying when the binding will not work.
 *
 * On Wayland the shortcut registers without complaining and then never fires,
 * so the only way the user finds out is if we say so.
 */
export function GlobalHotkey() {
  const qc = useQueryClient();
  const { data: hotkey } = useQuery({
    queryKey: ["hotkey"],
    queryFn: commands.getHotkey,
  });
  const { data: support } = useQuery({
    queryKey: ["hotkey-support"],
    queryFn: commands.hotkeySupport,
  });
  const register = useMutation({
    mutationFn: commands.registerHotkey,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["hotkey"] });
      qc.invalidateQueries({ queryKey: ["settings-snapshot"] });
    },
  });

  return (
    <div>
      <Explain>
        A modifier on its own works too — tap Ctrl, Alt or Shift and release it without pressing
        anything else. Held as part of a combination it behaves normally, so Ctrl+C is untouched.
      </Explain>
      <HotkeyCapture
        value={hotkey ?? ""}
        onChange={(accelerator) => register.mutate(accelerator)}
      />
      {register.isError ? <Problem>{errorMessage(register.error)}</Problem> : null}
      {support && support.advice !== "" ? (
        <p
          style={{
            margin: "var(--space-2) 0 0",
            color: "var(--text-tertiary)",
            fontSize: "var(--text-caption-size)",
            lineHeight: "var(--text-caption-line)",
            maxWidth: "58ch",
          }}
        >
          {support.advice}
        </p>
      ) : null}
    </div>
  );
}

const PILL_SIZES: { id: PillSize; title: string; sub: string; glyph: string }[] = [
  {
    id: "large",
    title: "Large",
    sub: "Level, elapsed time and settings, always visible",
    glyph: "h-3.5 w-14",
  },
  { id: "small", title: "Small", sub: "Level around the button's edge", glyph: "h-3.5 w-8" },
  { id: "line", title: "A line", sub: "Thin enough to ignore", glyph: "h-1 w-14" },
];

/**
 * The pill's three sizes, drawn to scale with each other so the choice is
 * legible before you make it.
 */
export function PillSizePicker() {
  const qc = useQueryClient();
  const { data: stored } = useQuery({
    queryKey: ["setting", "pill_size"],
    queryFn: () => commands.getSetting("pill_size"),
  });
  const active: PillSize = stored === "small" || stored === "line" ? stored : "large";

  const set = useMutation({
    // The pill is a separate webview with its own store, so persisting the
    // choice is not enough — it has to be told.
    mutationFn: async (v: PillSize) => {
      await commands.setSetting("pill_size", v);
      await echoEvents.emitPillSizeChanged(v);
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["setting", "pill_size"] });
      qc.invalidateQueries({ queryKey: ["settings-snapshot"] });
    },
  });

  return (
    <div>
      <Explain>
        The floating control you dictate from — drag it anywhere and Echo puts it back there next
        launch. All three show the same live level, with less and less of the pill around it.
      </Explain>
      <div role="radiogroup" aria-label="Pill size" className="grid grid-cols-3 gap-2.5">
        {PILL_SIZES.map((option) => {
          const selected = option.id === active;
          return (
            <button
              key={option.id}
              type="button"
              role="radio"
              aria-checked={selected}
              onClick={() => set.mutate(option.id)}
              data-selected={selected}
              className="material interactive focus-ring flex flex-col items-center gap-2"
              style={{
                padding: "var(--space-4) var(--space-3)",
                borderRadius: "var(--radius-input)",
                borderColor: selected ? "var(--accent)" : "var(--border-hairline)",
                cursor: "pointer",
              }}
            >
              <span
                aria-hidden
                className={`rounded-full ${option.glyph}`}
                style={{ background: selected ? "var(--accent)" : "var(--text-tertiary)" }}
              />
              <span
                style={{
                  color: "var(--text-primary)",
                  fontSize: "var(--text-label-size)",
                  fontWeight: "var(--text-label-weight)",
                }}
              >
                {option.title}
              </span>
              <span
                style={{
                  color: "var(--text-tertiary)",
                  fontSize: "var(--text-caption-size)",
                  lineHeight: "var(--text-caption-line)",
                  textAlign: "center",
                }}
              >
                {option.sub}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
