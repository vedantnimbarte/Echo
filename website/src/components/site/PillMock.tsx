"use client";

import { useEffect, useRef } from "react";
import useOnScreen from "@/components/ui/useOnScreen";

/**
 * Echo's actual surface: a frameless pill pinned to the bottom of the screen.
 * The bars are driven by one rAF writing heights directly, the way the real
 * one is driven by the capture task's RMS events.
 */
export default function PillMock({ className = "" }: { className?: string }) {
  const bars = useRef<(HTMLSpanElement | null)[]>([]);
  const [host, onScreen] = useOnScreen<HTMLDivElement>();

  useEffect(() => {
    if (!onScreen) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
      bars.current.forEach((b, i) => {
        if (b) b.style.height = `${20 + ((i * 37) % 60)}%`;
      });
      return;
    }
    let raf = 0;
    const start = performance.now();
    const tick = (now: number) => {
      const t = (now - start) / 1000;
      bars.current.forEach((b, i) => {
        if (!b) return;
        const v =
          Math.sin(t * 5.2 + i * 0.75) * 0.5 +
          Math.sin(t * 2.1 + i * 1.9) * 0.32 +
          Math.sin(t * 9.4 + i * 0.4) * 0.18;
        b.style.height = `${18 + Math.abs(v) * 74}%`;
      });
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [onScreen]);

  return (
    <div
      ref={host}
      className={`glass glow-ring inline-flex items-center gap-3 rounded-full px-4 py-2.5 ${className}`}
      role="img"
      aria-label="Echo's floating pill, listening"
    >
      <span className="h-2 w-2 shrink-0 rounded-full bg-ember" />
      <span className="flex h-6 items-center gap-[3px]">
        {Array.from({ length: 22 }).map((_, i) => (
          <span
            key={i}
            ref={(el) => {
              bars.current[i] = el;
            }}
            className="w-[2px] rounded-full bg-glow"
            style={{ height: "30%" }}
          />
        ))}
      </span>
      <span className="font-mono text-xs tabular-nums text-fog">0:07</span>
    </div>
  );
}
