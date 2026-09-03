"use client";

import { useEffect, useRef } from "react";
import { now, score, THRESHOLD } from "./wakeCycle";
import useOnScreen from "@/components/ui/useOnScreen";

/**
 * The classifier's readout, in DOM so the numbers stay crisp. It runs off the
 * same clock as the terrain, and writes straight to the nodes — a 60fps number
 * has no business going through React state.
 */
export default function DetectionMeter({ className = "" }: { className?: string }) {
  const bar = useRef<HTMLDivElement>(null);
  const value = useRef<HTMLSpanElement>(null);
  const state = useRef<HTMLDivElement>(null);
  const [host, onScreen] = useOnScreen<HTMLDivElement>();

  useEffect(() => {
    if (!onScreen) return;
    const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let raf = 0;

    const paint = (s: number) => {
      const hit = s >= THRESHOLD;
      if (bar.current) {
        bar.current.style.transform = `scaleX(${s})`;
        bar.current.style.background = hit
          ? "var(--color-ember)"
          : "var(--color-glow)";
      }
      if (value.current) value.current.textContent = s.toFixed(2);
      if (state.current) state.current.dataset.hit = String(hit);
    };

    if (reduced) {
      paint(0.86);
      return;
    }

    const tick = () => {
      paint(score(now()));
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [onScreen]);

  return (
    <div ref={host} className={`glass w-full max-w-xs rounded-2xl p-4 ${className}`}>
      <div className="flex items-baseline justify-between gap-4">
        <span className="datum uppercase tracking-[0.24em]">wake phrase</span>
        <span className="font-mono text-xs text-text">&ldquo;hey jarvis&rdquo;</span>
      </div>

      <div className="mt-4 flex items-center justify-between">
        <span className="datum">score</span>
        <span ref={value} className="font-mono text-sm tabular-nums text-text">
          0.04
        </span>
      </div>

      {/* meter, with the ship-default threshold marked on it */}
      <div className="relative mt-2 h-1.5 overflow-hidden rounded-full bg-ink-3">
        <div
          ref={bar}
          className="h-full w-full origin-left rounded-full"
          style={{ transform: "scaleX(0.04)", background: "var(--color-glow)" }}
        />
        <div
          className="absolute inset-y-0 w-px bg-fog/70"
          style={{ left: `${THRESHOLD * 100}%` }}
          aria-hidden
        />
      </div>
      <div className="mt-1.5 flex justify-between">
        <span className="datum">0.0</span>
        <span className="datum">threshold {THRESHOLD.toFixed(1)}</span>
      </div>

      <div
        ref={state}
        data-hit="false"
        className="group mt-4 flex items-center gap-2 border-t border-line pt-3 font-mono text-xs"
      >
        <span className="h-1.5 w-1.5 rounded-full bg-fog anim-pulse group-data-[hit=true]:animate-none group-data-[hit=true]:bg-ember" />
        <span className="text-fog group-data-[hit=true]:hidden">
          listening — nothing sent
        </span>
        <span className="hidden text-ember group-data-[hit=true]:inline">
          detected — dictating
        </span>
      </div>
    </div>
  );
}
