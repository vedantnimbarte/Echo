"use client";

import { useRef } from "react";

/**
 * A card that turns to face the pointer. Small angles on purpose — enough to
 * feel like a physical object catching the light, not a novelty.
 */
export default function TiltCard({
  children,
  className = "",
  max = 6,
}: {
  children: React.ReactNode;
  className?: string;
  max?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);

  const onMove = (e: React.PointerEvent) => {
    const el = ref.current;
    if (!el || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const r = el.getBoundingClientRect();
    const x = (e.clientX - r.left) / r.width - 0.5;
    const y = (e.clientY - r.top) / r.height - 0.5;
    el.style.transform = `rotateX(${-y * max}deg) rotateY(${x * max}deg) translateZ(6px)`;
  };

  const reset = () => {
    if (ref.current) ref.current.style.transform = "";
  };

  return (
    <div className="stage h-full">
      <div
        ref={ref}
        onPointerMove={onMove}
        onPointerLeave={reset}
        className={`card-3d h-full ${className}`}
      >
        {children}
      </div>
    </div>
  );
}
