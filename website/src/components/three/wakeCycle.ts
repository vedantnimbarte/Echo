/**
 * The hero's detection clock.
 *
 * The 3D terrain and the DOM score readout both animate off this, so they stay
 * in step without React state churn — each reads the same function from its own
 * frame loop.
 */

const ORIGIN = typeof performance === "undefined" ? 0 : performance.now();

/**
 * Seconds since the page loaded. Both readouts take their time from here, so
 * pausing one of them offscreen never drifts it out of step with the other.
 */
export const now = () => (performance.now() - ORIGIN) / 1000;

/** Seconds between one detection and the next. */
export const CYCLE = 9.5;

/** Seconds the detected ridge takes to travel from the far edge to the near one. */
export const ATTACK = 1.9;

/** The wake-word classifier's threshold, as Echo ships it. */
export const THRESHOLD = 0.5;

const smoothstep = (a: number, b: number, x: number) => {
  const t = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

/** 0 while idle, 0 → 1 as the ridge crosses the terrain. */
export function phase(t: number) {
  const p = t % CYCLE;
  return p < ATTACK ? p / ATTACK : 0;
}

/** Bell-shaped confidence envelope for one detection. */
export function envelope(t: number) {
  const p = phase(t);
  if (p === 0) return 0;
  return smoothstep(0, 0.3, p) * (1 - smoothstep(0.68, 1, p));
}

/** Score 0..1, the number the phrase classifier would emit. */
export function score(t: number) {
  return 0.04 + 0.92 * envelope(t);
}
