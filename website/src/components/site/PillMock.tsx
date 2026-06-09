"use client";

import { motion } from "motion/react";

const BARS = [0.4, 0.85, 0.55, 1, 0.65, 0.3, 0.75, 0.5, 0.9, 0.45, 0.7, 0.35];

/**
 * A faithful mock of Echo's floating pill — the surface users actually see
 * while dictating. Live equalizer bars + recording state.
 */
export default function PillMock({
  state = "listening",
  className = "",
}: {
  state?: "listening" | "done";
  className?: string;
}) {
  return (
    <div
      className={`glass glow-ring inline-flex items-center gap-3 rounded-full px-5 py-3 ${className}`}
    >
      <span className="relative flex h-2.5 w-2.5">
        <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-glow opacity-60" />
        <span className="relative inline-flex h-2.5 w-2.5 rounded-full bg-glow" />
      </span>

      <div className="flex h-6 items-center gap-[3px]">
        {BARS.map((b, i) => (
          <motion.span
            key={i}
            className="w-[3px] rounded-full bg-gradient-to-t from-glow to-mint"
            animate={{ scaleY: state === "done" ? 0.18 : [b * 0.4, b, b * 0.5] }}
            transition={{
              duration: 0.9 + (i % 4) * 0.18,
              repeat: state === "done" ? 0 : Infinity,
              repeatType: "mirror",
              ease: "easeInOut",
            }}
            style={{ height: 22, transformOrigin: "center" }}
          />
        ))}
      </div>

      <span className="font-mono text-xs tracking-wide text-fog">
        {state === "done" ? "transcribed" : "0:04"}
      </span>
    </div>
  );
}
