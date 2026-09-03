/**
 * The settings window's page shell.
 *
 * Every panel is one topic on one page now, so the glass card that used to
 * frame each section was drawing a box around the only thing on screen.
 * Structure comes from the baseline instead: a page header, then groups
 * separated by hairlines. Glass is reserved for things you actually act on —
 * a model, a profile, a plugin — so it reads as "object" rather than "region".
 *
 * Type sizes are unchanged from the old panels on purpose; the room comes from
 * spacing, not from scaling everything up.
 */

export function Page({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: string;
  /** Page-level controls, aligned to the title's baseline. */
  actions?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="mx-auto w-full max-w-[620px] px-10 py-9">
      <header className="mb-7 flex items-start justify-between gap-6">
        <div className="min-w-0">
          <h2 className="text-[15px] font-semibold tracking-tight text-[var(--ink)]">
            {title}
          </h2>
          {description && (
            <p className="mt-1.5 max-w-[52ch] text-[11.5px] leading-relaxed text-[var(--ink-muted)]">
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
 * One labelled group of controls. `hint` sits at the bottom because it explains
 * the group after you've seen it, not before.
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
    <section className="space-y-3.5 py-5 first:pt-0 last:pb-0">
      {title && (
        <h3 className="text-[11px] font-medium uppercase tracking-[0.09em] text-[var(--ink-faint)]">
          {title}
        </h3>
      )}
      {children}
      {hint && (
        <p className="max-w-[56ch] text-[10.5px] leading-relaxed text-[var(--ink-faint)]">
          {hint}
        </p>
      )}
    </section>
  );
}

/** A single labelled control inside a group. */
export function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <label className={"block space-y-1.5 " + (className ?? "")}>
      <span className="text-[11px] font-medium text-[var(--ink-muted)]">{label}</span>
      {children}
    </label>
  );
}

/** A checkbox and its sentence, aligned so the text reads as the label. */
export function Check({
  checked,
  onChange,
  children,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  children: React.ReactNode;
}) {
  return (
    <label className="flex items-start gap-2.5">
      <input
        type="checkbox"
        className="mt-px h-4 w-4 shrink-0 accent-white"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <span className="text-[12px] leading-snug text-[var(--ink)]">{children}</span>
    </label>
  );
}
