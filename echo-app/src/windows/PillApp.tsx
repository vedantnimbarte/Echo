import { useEffect } from "react";
import {
  getCurrentWindow,
  currentMonitor,
  primaryMonitor,
  PhysicalPosition,
} from "@tauri-apps/api/window";

import { useEchoEvents } from "../hooks/useEchoEvents";
import { Pill } from "../components/pill/Pill";

/**
 * Pin the frameless pill to the bottom-center of the active monitor, sitting a
 * little above the taskbar/dock.
 */
async function placeBottomCenter() {
  const win = getCurrentWindow();
  const mon = (await currentMonitor()) ?? (await primaryMonitor());
  if (!mon) return;
  const size = await win.outerSize();
  const x = Math.round(mon.position.x + (mon.size.width - size.width) / 2);
  const margin = Math.round(56 * mon.scaleFactor);
  const y = Math.round(mon.position.y + mon.size.height - size.height - margin);
  await win.setPosition(new PhysicalPosition(x, y));
}

export function PillApp() {
  // The pill is the single owner of the global-hotkey toggle.
  useEchoEvents({ controlHotkey: true });

  useEffect(() => {
    void placeBottomCenter();
  }, []);

  return <Pill />;
}
