import { cloneElement, isValidElement, useId } from "react";

import { Hint } from "./Hint";

/**
 * The settings window's page shell.
 *
 * Every panel is one topic on one page, so the glass card that used to frame
 * each section was drawing a box around the only thing on screen. Structure
 * comes from the baseline instead: a page header, then groups separated by
 * hairlines. Glass is reserved for things you actually act on — a model, a
 * profile, a plugin — so it reads as "object" rather than "region".
 *
 * The title is the one place Garamond appears, and it appears large. Everything
 * below it is Figtree at operating sizes: a page you read at the top and use
 * underneath.
 *
 * Room comes from spacing, not from scaling everything up — and from what is no
 * longer here. The paragraph that used to explain each group now lives behind
 * its `hint`, one hover away.
 */

export function Page({
  title,
  description,
  actions,
  /**
   * Measure, in px. Settings pages are a column of controls and read best
   * narrow; a page of charts needs the room, and cramming one into 640 would
   * stack cards that are meant to be compared side by side.
   */
  width = 640,
  children,
}: {
  /**
   * Usually the page's name. A node when the heading has something set into it
   * — Dictation puts the keys of your shortcut in its own, which is why this is
   * not a plain string. Whatever is passed lands in the page's one `h2`, so a
   * page always has a heading to be found by.
   */
  title: React.ReactNode;
  description?: React.ReactNode;
  /** Page-level controls, aligned to the title's baseline. */
  actions?: React.ReactNode;
  width?: number;
  children: React.ReactNode;
}) {
  return (
    <div className="mx-auto w-full px-12 py-12" style={{ maxWidth: width }}>
      <header className="mb-9 flex items-start justify-between gap-6">
        <div className="min-w-0">
          <h2 className="display text-[30px] text-[var(--ink)]">{title}</h2>
          {description && (
            <p className="mt-2 max-w-[46ch] text-[14.5px] leading-relaxed text-[var(--ink-muted)]">
              {description}
            </p>
          )}
        </div>
        {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
      </header>
      {/* divide-y draws rules only *between* groups, so no first/last-child
          padding fights with the group's own spacing. */}
      <div className="divide-y divide-[var(--hairline)]">{children}</div>
    </div>
  );
}

/**
 * One labelled group of controls.
 *
 * `hint` is the group's explanation. It sits behind an info icon on the title
 * rather than as a paragraph underneath, because it answers a question you have
 * once and then never again.
 */
export function Group({
  title,
  hint,
  children,
}: {
  title?: string;
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-4 py-7 first:pt-0 last:pb-0">
      {title ? (
        <h3 className="flex items-center gap-1.5 text-[14.5px] font-medium tracking-tight text-[var(--ink)]">
          {title}
          {hint && <Hint label={`About ${title.toLowerCase()}`}>{hint}</Hint>}
        </h3>
      ) : (
        // A hint with nothing to hang off still needs somewhere to sit.
        hint && <Hint>{hint}</Hint>
      )}
      {children}
    </section>
  );
}

/**
 * A single labelled control inside a group.
 *
 * Without a hint the label simply wraps its control, which is what associates
 * the two. A hint can't be wrapped that way — the icon is a button, and a
 * button inside a label is both invalid and ambiguous to click — so the hinted
 * form associates by id instead, and falls back to wrapping if the caller
 * passed something other than a single element.
 */
export function Field({
  label,
  hint,
  children,
  className,
}: {
  label: string;
  /** The "why" for this one control, shown on hover or focus of its icon. */
  hint?: React.ReactNode;
  children: React.ReactNode;
  className?: string;
}) {
  const id = useId();
  // `block`, because an inline span lets the control ride up beside its own
  // label — which is how these read before, and why "Method" and "Insert delay"
  // started at different left edges.
  const labelClass = "block text-[13.5px] font-medium text-[var(--ink-muted)]";
  const control = isValidElement<{ id?: string }>(children);

  if (!hint || !control) {
    return (
      <label className={"block space-y-2 " + (className ?? "")}>
        <span className={labelClass}>{label}</span>
        {children}
      </label>
    );
  }

  return (
    <div className={"space-y-2 " + (className ?? "")}>
      <div className="flex items-center gap-1.5">
        <label htmlFor={id} className={labelClass}>
          {label}
        </label>
        <Hint label={`About ${label.toLowerCase()}`}>{hint}</Hint>
      </div>
      {cloneElement(children, { id })}
    </div>
  );
}

/** A checkbox and its sentence, aligned so the text reads as the label. */
export function Check({
  checked,
  onChange,
  hint,
  children,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  /** The "why" for this switch, shown on hover or focus of its icon. */
  hint?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start gap-1.5">
      <label className="flex items-start gap-2.5">
        <input
          type="checkbox"
          className="mt-px h-4 w-4 shrink-0 accent-white"
          checked={checked}
          onChange={(e) => onChange(e.target.checked)}
        />
        <span className="text-[14.5px] leading-snug text-[var(--ink)]">{children}</span>
      </label>
      {hint && <Hint>{hint}</Hint>}
    </div>
  );
}
