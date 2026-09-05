import { useEffect, useRef, useState } from "react";
import {
  getCurrentWindow,
  currentMonitor,
  primaryMonitor,
  availableMonitors,
  LogicalSize,
  PhysicalPosition,
  type Monitor,
} from "@tauri-apps/api/window";

import { useEchoEvents } from "../hooks/useEchoEvents";
import { commands } from "../ipc/commands";
import { echoEvents } from "../ipc/events";
import { Pill, type PillSize } from "../components/pill/Pill";
import { useRecordingStore } from "../store/recordingStore";
import { cueStart, cueStop } from "../lib/cues";

/**
 * Window footprint per variant, in logical px. Wider than the capsule it holds:
 * the shell casts a soft shadow and, while live, a red bloom that would be
 * clipped at the window edge.
 */
const FOOTPRINT: Record<PillSize, { width: number; height: number }> = {
  large: { width: 360, height: 80 },
  small: { width: 160, height: 80 },
};

/** Gap between the pill and the bottom of the screen, in logical px. */
const BOTTOM_MARGIN = 56;

/** Where the user parked it, as physical `"x,y"` of the window's centre. */
const POSITION_KEY = "pill_position";

/**
 * The saved point is the window's *centre*, not its top-left corner.
 *
 * The two variants are different widths, so storing a corner would shift the
 * pill sideways every time you switched size. A centre keeps it visually put.
 */
function parseCentre(raw: string | null): { x: number; y: number } | null {
  if (!raw) return null;
  const [x, y] = raw.split(",").map(Number);
  return Number.isFinite(x) && Number.isFinite(y) ? { x, y } : null;
}

function monitorContaining(monitors: Monitor[], p: { x: number; y: number }) {
  return monitors.find(
    (m) =>
      p.x >= m.position.x &&
      p.x < m.position.x + m.size.width &&
      p.y >= m.position.y &&
      p.y < m.position.y + m.size.height
  );
}

/**
 * Size the frameless window to the variant, then put it where the user left it
 * — or bottom-centre if they never moved it.
 *
 * Position is computed from the logical size rather than read back with
 * `outerSize()`: the resize has not necessarily reached the OS by the time the
 * promise resolves, and placing against a stale size makes the pill jump.
 */
async function place(size: PillSize) {
  const win = getCurrentWindow();
  const { width, height } = FOOTPRINT[size];
  await win.setSize(new LogicalSize(width, height));

  const saved = parseCentre(await commands.getSetting(POSITION_KEY));
  const monitors = await availableMonitors();

  // A position saved on a monitor that is no longer attached would put the pill
  // somewhere unreachable, so fall back to the default rather than trusting it.
  const home = (saved && monitorContaining(monitors, saved)) ?? null;
  const mon = home ?? (await currentMonitor()) ?? (await primaryMonitor());
  if (!mon) return;

  const sf = mon.scaleFactor;
  const w = Math.round(width * sf);
  const h = Math.round(height * sf);

  let x: number;
  let y: number;
  if (home && saved) {
    x = Math.round(saved.x - w / 2);
    y = Math.round(saved.y - h / 2);
  } else {
    x = Math.round(mon.position.x + (mon.size.width - w) / 2);
    y = Math.round(mon.position.y + mon.size.height - h - BOTTOM_MARGIN * sf);
  }

  // Dragging is free-form, so the pill can be left hanging over an edge. Pull
  // it fully back on screen when it is next placed — a sliver of pill is not
  // something you can grab to fix.
  x = Math.min(Math.max(x, mon.position.x), mon.position.x + mon.size.width - w);
  y = Math.min(Math.max(y, mon.position.y), mon.position.y + mon.size.height - h);

  await win.setPosition(new PhysicalPosition(x, y));
}

export function PillApp() {
  // The pill is the single owner of the global-hotkey toggle.
  useEchoEvents({ controlHotkey: true });

  const [size, setSize] = useState<PillSize>("large");
  // The variant is only known after the setting loads. Placing before then
  // would size the window to the default first and snap a moment later.
  const [sizeLoaded, setSizeLoaded] = useState(false);
  const [cues, setCues] = useState(false);
  // Only a move the user started should overwrite the saved position — our own
  // placement calls fire `onMoved` too. Set on the first drag and left set:
  // every later programmatic placement resolves to the same centre anyway.
  const userMoved = useRef(false);

  // The setting is written in the settings window, which is a separate webview
  // with its own store — so read it once here and take live changes over the
  // event bus.
  useEffect(() => {
    void commands.getSetting("pill_size").then((v) => {
      setSize(v === "small" ? "small" : "large");
      setSizeLoaded(true);
    });

    let unlisten: (() => void) | undefined;
    void echoEvents.onPillSizeChanged(setSize).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  useEffect(() => {
    if (sizeLoaded) void place(size);
  }, [size, sizeLoaded]);

  // Audio cues. The pill is often not where the user is looking — that is the
  // point of dictating into another app — so a sound is the only feedback that
  // reliably lands. Read once and kept live over the event bus, the same way
  // the size setting is, because the toggle lives in the other webview.
  useEffect(() => {
    void commands.getSetting("sound_cues").then((v) => setCues(v === "true"));
  }, []);

  const isRecording = useRecordingStore((s) => s.isRecording);
  const wasRecording = useRef(false);
  useEffect(() => {
    if (!cues) {
      wasRecording.current = isRecording;
      return;
    }
    // Edge-triggered: the store updates on unrelated fields too, and a cue on
    // every render would be unbearable.
    if (isRecording && !wasRecording.current) cueStart();
    if (!isRecording && wasRecording.current) cueStop();
    wasRecording.current = isRecording;
  }, [isRecording, cues]);

  // Remember where it was dropped. `onMoved` fires continuously while dragging,
  // so settle first and write once.
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let settle: ReturnType<typeof setTimeout> | undefined;

    void win
      .onMoved(({ payload }) => {
        if (!userMoved.current) return;
        clearTimeout(settle);
        settle = setTimeout(() => {
          void win.outerSize().then((s) =>
            commands.setSetting(
              POSITION_KEY,
              `${Math.round(payload.x + s.width / 2)},${Math.round(payload.y + s.height / 2)}`
            )
          );
        }, 400);
      })
      .then((fn) => {
        unlisten = fn;
      });

    return () => {
      clearTimeout(settle);
      unlisten?.();
    };
  }, []);

  return <Pill size={size} onDragStart={() => (userMoved.current = true)} />;
}
