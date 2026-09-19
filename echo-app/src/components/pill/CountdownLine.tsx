/**
 * SOURCE OF TRUTH KEYWORDS: CountdownLine, cancel-armed, draining line
 * WHAT:  A line that starts full and drains to nothing over the cancel window.
 * WHY:   It replaces the waveform in the pill's centre slot rather than sitting
 *        beside it, which is what makes the swap read as one object changing
 *        mode instead of two things fighting over the same space.
 *
 *        THE FORM CARRIES THE STATE, not a colour. A moving waveform means
 *        recording; a draining line means cancelling. That is the palette's
 *        rule (there is no accent hue to spend) and it is also the more legible
 *        signal — the line says how long is left, which a colour cannot.
 *
 *        The duration comes in as a prop from the backend event rather than
 *        being a constant here, because the countdown that actually expires is
 *        the one Rust is timing. Two copies of that number would drift and the
 *        animation would finish before or after the thing it is animating.
 *
 *        Animated by CSS, not by a React interval: this renders while an audio
 *        thread and a decoder are both running, and the pill has a hard 60fps
 *        budget. A transform the compositor owns costs no JavaScript frames.
 * WHERE: Rendered by Pill in place of the waveform while cancel is armed.
 */

export function CountdownLine({ durationMs }: { durationMs: number }) {
  return (
    <div
      className="relative w-full overflow-hidden rounded-full"
      style={{
        height: "var(--countdown-line-height)",
        background: "var(--surface-sunken-strong)",
      }}
      role="progressbar"
      aria-label="Cancelling — press Escape again to keep recording"
    >
      <div
        className="absolute inset-y-0 left-0 w-full origin-left rounded-full"
        style={{
          background: "var(--accent)",
          animation: `echo-countdown ${durationMs}ms linear forwards`,
        }}
      />
      {/* Scoped here rather than in styles.css: it is the only user of this
          keyframe, and a global animation name is a thing to collide with. */}
      <style>{`
        @keyframes echo-countdown {
          from { transform: scaleX(1); }
          to   { transform: scaleX(0); }
        }
      `}</style>
    </div>
  );
}
