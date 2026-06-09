import { useEffect, useRef } from "react";
import { useAudioLevel } from "../../hooks/useAudioLevel";

export type WaveMode = "idle" | "listening" | "transcribing";

const BARS = 22;

/**
 * A row of vertical bars driven by a single rAF loop. In `listening` mode the
 * bars track the live captured-audio ring buffer; `transcribing` plays a
 * traveling pulse; `idle` breathes gently. Heights are eased toward their
 * target each frame and written straight to the DOM (no React churn).
 */
export function Waveform({ mode }: { mode: WaveMode }) {
  const levels = useAudioLevel(BARS);
  const containerRef = useRef<HTMLDivElement>(null);
  const heights = useRef<Float32Array>(new Float32Array(BARS).fill(0.12));
  const modeRef = useRef<WaveMode>(mode);
  modeRef.current = mode;

  // When the OS asks to reduce motion we keep the live meter (it's essential
  // feedback) but drop the synthetic idle "breathing" and transcribing sweep.
  const reduced = useRef(
    typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches
  );

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const bars = Array.from(el.children) as HTMLElement[];
    let raf = 0;
    let startTs = 0;

    const tick = (now: number) => {
      if (!startTs) startTs = now;
      const t = (now - startTs) / 1000;
      const cur = heights.current;
      const m = modeRef.current;

      for (let i = 0; i < BARS; i++) {
        let target: number;
        if (m === "listening") {
          target = levels.current[i];
        } else if (reduced.current) {
          // Reduced motion: static bars for non-listening states.
          target = m === "transcribing" ? 0.4 : 0.12;
        } else if (m === "transcribing") {
          // A bright pulse sweeping left→right over a low rolling base.
          const head = (t * 13) % (BARS + 8);
          const pulse = Math.exp(-Math.abs(i - head) / 3.2);
          target = 0.18 + 0.7 * pulse + 0.08 * Math.sin(t * 6 - i * 0.5);
        } else {
          // Idle: a slow, shallow breath.
          target = 0.1 + 0.07 * (0.5 + 0.5 * Math.sin(t * 1.8 - i * 0.45));
        }
        // Snappy rise, gentler fall feels closest to a real meter.
        const k = target > cur[i] ? 0.4 : 0.16;
        cur[i] += (target - cur[i]) * k;
        const h = Math.max(0.05, Math.min(1, cur[i]));
        const bar = bars[i];
        bar.style.transform = `scaleY(${h.toFixed(3)})`;
        bar.style.opacity = (0.35 + 0.65 * h).toFixed(3);
      }
      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [levels]);

  return (
    <div
      ref={containerRef}
      className="flex h-5 items-center gap-[2px]"
      aria-hidden
    >
      {Array.from({ length: BARS }).map((_, i) => (
        <span
          key={i}
          className="h-full w-[2px] rounded-full"
          style={{
            transformOrigin: "center",
            background:
              "linear-gradient(to top, var(--aurora-1), var(--aurora-2) 55%, var(--aurora-3))",
          }}
        />
      ))}
    </div>
  );
}
