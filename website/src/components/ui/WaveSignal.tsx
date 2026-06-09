"use client";

import { useEffect, useRef } from "react";

type Props = {
  className?: string;
  height?: number;
  lines?: number;
  speed?: number;
  /** lower = calmer ambient motion */
  intensity?: number;
};

/**
 * The Echo signal. A sharp spoken spike on the left decays and collapses
 * into flowing, parallel lines on the right (voice → text). It breathes on
 * its own and bulges toward the pointer — the brand's living centerpiece.
 */
export default function WaveSignal({
  className = "",
  height = 320,
  lines = 5,
  speed = 1,
  intensity = 1,
}: Props) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

    let w = 0;
    let h = 0;
    let dpr = Math.min(window.devicePixelRatio || 1, 2);

    const resize = () => {
      const rect = canvas.getBoundingClientRect();
      w = rect.width;
      h = rect.height;
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.max(1, Math.floor(w * dpr));
      canvas.height = Math.max(1, Math.floor(h * dpr));
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };

    const ro = new ResizeObserver(resize);
    ro.observe(canvas);
    resize();

    // pointer interaction (eased)
    let pointerT = 0.5;
    let pointerActive = 0;
    let pointerTarget = 0;
    const onMove = (e: PointerEvent) => {
      const rect = canvas.getBoundingClientRect();
      pointerT = Math.min(1, Math.max(0, (e.clientX - rect.left) / rect.width));
      pointerTarget = 1;
    };
    const onLeave = () => {
      pointerTarget = 0;
    };
    canvas.addEventListener("pointermove", onMove);
    canvas.addEventListener("pointerleave", onLeave);

    const gauss = (t: number, c: number, s: number) =>
      Math.exp(-((t - c) * (t - c)) / (2 * s * s));

    // rounded spoken spike profile (signed, mostly active for t < 0.3).
    // gaussian bumps give soft, curved peaks instead of pointed ECG edges.
    const zig = (t: number) =>
      0.32 * gauss(t, 0.07, 0.017) +
      0.98 * gauss(t, 0.155, 0.013) -
      1.08 * gauss(t, 0.188, 0.014) +
      0.52 * gauss(t, 0.222, 0.012) -
      0.42 * gauss(t, 0.252, 0.012) +
      0.26 * gauss(t, 0.288, 0.013);

    const smooth = (a: number, b: number, x: number) => {
      const k = Math.min(1, Math.max(0, (x - a) / (b - a)));
      return k * k * (3 - 2 * k);
    };

    let raf = 0;
    let time = 0;
    const mid = (lines - 1) / 2;

    const draw = () => {
      time += 0.016 * speed;
      pointerActive += (pointerTarget - pointerActive) * 0.06;

      ctx.clearRect(0, 0, w, h);
      const center = h / 2;
      const amp = h * 0.32 * intensity;
      const gap = h * 0.072;
      const pulse = 0.82 + 0.18 * Math.sin(time * 1.25);

      const grad = ctx.createLinearGradient(0, 0, w, 0);
      grad.addColorStop(0, "rgba(79,240,230,0.95)");
      grad.addColorStop(0.5, "rgba(121,247,199,0.9)");
      grad.addColorStop(1, "rgba(88,196,255,0.55)");

      for (let li = 0; li < lines; li++) {
        const distFromMid = Math.abs(li - mid) / (mid || 1);
        const spikeStrength = 1 - distFromMid; // center line carries the spike
        const flowPhase = li * 1.15;

        // sample the signal, then stroke it as a smooth curve
        const step = 4;
        const pts: { x: number; y: number }[] = [];
        for (let px = 0; px <= w; px += step) {
          const t = px / w;
          const fan = (li - mid) * gap * smooth(0.42, 1, t);

          const rightEnv = smooth(0.26, 0.55, t) * (1 - 0.55 * smooth(0.72, 1, t));
          const flow =
            Math.sin(t * 22 + time * 1.7 + flowPhase) * 0.6 +
            Math.sin(t * 47 - time * 2.1 + li) * 0.34;

          // pointer bulge
          const bulge =
            gauss(t, pointerT, 0.07) * pointerActive * amp * 0.4 * (li % 2 ? -1 : 1);

          const spike =
            zig(t) * spikeStrength * pulse * (1 + pointerActive * 0.45);

          const y =
            center +
            fan -
            amp * spike +
            amp * 0.18 * flow * rightEnv +
            bulge;

          pts.push({ x: px, y });
        }

        ctx.beginPath();
        ctx.moveTo(pts[0].x, pts[0].y);
        // quadratic smoothing: curve through each point's midpoint so the
        // path flows with rounded joints instead of straight segments
        for (let i = 1; i < pts.length - 1; i++) {
          const mx = (pts[i].x + pts[i + 1].x) / 2;
          const my = (pts[i].y + pts[i + 1].y) / 2;
          ctx.quadraticCurveTo(pts[i].x, pts[i].y, mx, my);
        }
        const last = pts[pts.length - 1];
        ctx.lineTo(last.x, last.y);

        ctx.strokeStyle = grad;
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        ctx.lineWidth = li === Math.round(mid) ? 2.6 : 1.6;
        ctx.globalAlpha = li === Math.round(mid) ? 1 : 0.62;
        ctx.shadowColor = "rgba(79,240,230,0.7)";
        ctx.shadowBlur = li === Math.round(mid) ? 22 : 12;
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
      ctx.shadowBlur = 0;

      if (!reduce) raf = requestAnimationFrame(draw);
    };

    draw();

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      canvas.removeEventListener("pointermove", onMove);
      canvas.removeEventListener("pointerleave", onLeave);
    };
  }, [height, lines, speed, intensity]);

  return (
    <canvas
      ref={canvasRef}
      style={{ height }}
      className={`w-full ${className}`}
      aria-hidden
    />
  );
}
