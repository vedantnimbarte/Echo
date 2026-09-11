import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { Info } from "lucide-react";

/** Tooltip measure. Wide enough for two or three lines, short enough to read. */
const WIDTH = 300;
/** Breathing room between the icon and the panel, and from the window edge. */
const GAP = 8;
const MARGIN = 12;

/**
 * The "why" behind a control, one hover away.
 *
 * Settings pages used to carry a paragraph under every group explaining what
 * the control was for. Read once, then in the way forever — and with nineteen
 * of them the pages were mostly prose. The explanations are still worth having,
 * so they moved here: an icon you can ignore until the moment you can't.
 *
 * Hover is not the only way in. The icon is a real button, so it takes keyboard
 * focus and answers Enter; Escape dismisses. Anyone who can't hover can still
 * tap it.
 *
 * The panel is positioned fixed and measured at open time rather than nested in
 * the flow, because every page here lives inside an overflow-y-auto main and an
 * absolutely positioned tooltip would be clipped at its edge.
 */
export function Hint({
  children,
  label = "What this does",
}: {
  children: React.ReactNode;
  /** Overrides the icon's accessible name where "what this does" is vague. */
  label?: string;
}) {
  const id = useId();
  const anchor = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const [at, setAt] = useState<{ top: number; left: number } | null>(null);

  const open = () => {
    const r = anchor.current?.getBoundingClientRect();
    if (!r) return;
    setAt({
      top: r.bottom + GAP,
      // Clamp before paint so a hint near the right edge never opens offscreen.
      left: Math.min(Math.max(r.left, MARGIN), window.innerWidth - WIDTH - MARGIN),
    });
  };
  const close = () => setAt(null);

  // Flip above the icon when the panel would run off the bottom. Its height
  // isn't knowable until it has rendered, so this corrects in the same frame
  // rather than guessing a line count up front.
  useLayoutEffect(() => {
    if (!at || !panel.current || !anchor.current) return;
    const h = panel.current.offsetHeight;
    if (at.top + h + MARGIN <= window.innerHeight) return;
    const r = anchor.current.getBoundingClientRect();
    setAt({ top: r.top - h - GAP, left: at.left });
  }, [at]);

  // Fixed coordinates go stale the moment the page moves under them, so a
  // scroll dismisses rather than leaving the panel pointing at nothing.
  useEffect(() => {
    if (!at) return;
    window.addEventListener("scroll", close, true);
    return () => window.removeEventListener("scroll", close, true);
  }, [at]);

  return (
    <>
      <button
        ref={anchor}
        type="button"
        aria-label={label}
        aria-expanded={at !== null}
        aria-describedby={at ? id : undefined}
        onMouseEnter={open}
        onMouseLeave={close}
        onFocus={open}
        onBlur={close}
        onClick={() => (at ? close() : open())}
        onKeyDown={(e) => e.key === "Escape" && close()}
        className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
      >
        <Info className="h-[13px] w-[13px]" />
      </button>

      {at && (
        <div
          ref={panel}
          id={id}
          role="tooltip"
          // Solid, not glass. Every other floating surface in Echo is
          // translucent, but this one lands on top of whatever you were reading
          // and has to win against it — a hint you can see the page through is
          // a hint you have to squint at.
          style={{
            position: "fixed",
            top: at.top,
            left: at.left,
            width: WIDTH,
            background: "var(--popup)",
            boxShadow: "var(--shadow-lg), var(--edge-light)",
          }}
          className="animate-hint z-50 rounded-xl border border-[var(--hairline)] px-3.5 py-2.5 text-[11.5px] leading-[1.55] text-[var(--ink-muted)]"
        >
          {children}
        </div>
      )}
    </>
  );
}
