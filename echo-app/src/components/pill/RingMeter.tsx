import { useEffect, useRef } from "react";
import { useAudioLevel } from "../../hooks/useAudioLevel";

export type RingMode = "idle" | "listening" | "transcribing";

/** Geometry of the small pill's button, in px. */
const SIZE = 44;
const STROKE = 2;
const R = (SIZE - STROKE) / 2 - 1;
const C = 2 * Math.PI * R;

/**
 * The small pill's level meter, drawn as the button's own ring.
 *
 * The large pill spends a whole capsule on a row of bars; the small one has
 * only its edge, so the edge becomes the instrument. The arc grows clockwise
 * from twelve o'clock with the captured level, which is the same reading as
 * the bar meter wrapped around a circle — nothing new to learn between the two
 * variants.
 *
 * Driven straight to the DOM from one rAF loop, like `Waveform`: at 60fps this
 * would otherwise be 60 React renders a second for one number.
 */
export function RingMeter({ mode, live }: { mode: RingMode; live: boolean }) {
  // One slot is all we need — the newest sample, not a scrolling history.
  const levels = useAudioLevel(1);
  const arcRef = useRef<SVGCircleElement>(null);
  const modeRef = useRef<RingMode>(mode);
  modeRef.current = mode;

  // Reduced motion keeps the live level (it is essential feedback) but not the
  // synthetic transcribing sweep — same rule the bar meter follows.
  const reduced = useRef(
    typeof matchMedia === "function" &&
      matchMedia("(prefers-reduced-motion: reduce)").matches
  );

  useEffect(() => {
    const arc = arcRef.current;
    if (!arc) return;
    let raf = 0;
    let startTs = 0;
    let shown = 0;

    const tick = (now: number) => {
      if (!startTs) startTs = now;
      const m = modeRef.current;

      let target: number;
      let rotation = -90; // start the arc at twelve o'clock
      if (m === "listening") {
        target = levels.current[0];
      } else if (m === "transcribing") {
        // A quarter-arc that travels round while Whisper works.
        target = 0.25;
        if (!reduced.current) rotation += ((now - startTs) / 1000) * 260;
      } else {
        target = 0;
      }

      // Snappy rise, gentler fall — a meter, not a slider.
      shown += (target - shown) * (target > shown ? 0.4 : 0.16);
      arc.style.strokeDashoffset = `${C * (1 - Math.min(1, Math.max(0, shown)))}`;
      arc.style.transform = `rotate(${rotation}deg)`;
      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [levels]);

  return (
    <svg
      className="pointer-events-none absolute inset-0"
      width={SIZE}
      height={SIZE}
      viewBox={`0 0 ${SIZE} ${SIZE}`}
      aria-hidden
    >
      {/* The track: always there, so the ring reads as an instrument at rest
          rather than as something that appeared. */}
      <circle
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={R}
        fill="none"
        stroke="var(--hairline-strong)"
        strokeWidth={1}
      />
      <circle
        ref={arcRef}
        cx={SIZE / 2}
        cy={SIZE / 2}
        r={R}
        fill="none"
        stroke={live ? "var(--rec)" : "var(--ink)"}
        strokeWidth={STROKE}
        strokeLinecap="round"
        strokeDasharray={C}
        strokeDashoffset={C}
        style={{ transformOrigin: "center", transition: "stroke 0.3s ease" }}
      />
    </svg>
  );
}
