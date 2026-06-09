import { useEffect, useRef } from "react";
import { echoEvents } from "../ipc/events";

/**
 * Subscribes to the backend's per-chunk RMS stream and maintains a left-
 * scrolling ring buffer of perceptually-normalized levels (newest at the
 * right). Returns a stable ref so the waveform renderer can read it inside a
 * requestAnimationFrame loop without forcing React re-renders.
 */
export function useAudioLevel(bars: number) {
  const levels = useRef<Float32Array>(new Float32Array(bars));

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void echoEvents
      .onAudioLevel((raw) => {
        // RMS sits low (~0–0.3 for speech). Lift with gain, then a gamma curve
        // so quiet speech still registers without clipping loud peaks.
        const n = Math.min(1, Math.pow(Math.max(0, raw) * 7, 0.7));
        const buf = levels.current;
        buf.copyWithin(0, 1);
        buf[buf.length - 1] = n;
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, [bars]);

  return levels;
}
