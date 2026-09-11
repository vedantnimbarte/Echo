import { useQuery } from "@tanstack/react-query";

import { commands, type DayWords, type Tally } from "../../ipc/commands";
import { Page, Group } from "../common/Page";

/**
 * What dictation has added up to.
 *
 * Every number here is counted from History, because History already is the
 * record — a second tally kept somewhere else would be one more thing to keep
 * in step with it, and one more thing the user cannot see or delete. The
 * visible cost is that turning History off empties this page, and the page
 * says so rather than showing zeroes that look like a bug.
 *
 * Monolith has no hue to spend, so the charts are built out of light instead:
 * every bar, cell and arc is white at some opacity, and "more" simply means
 * "brighter". The one colour in the app, --rec, is deliberately absent here —
 * it means "the microphone is live" and nothing else, and a red bar in a chart
 * would quietly spend that meaning.
 */

/** Words per minute, typing. A competent touch-typist composing prose. */
const TYPING_WPM = 45;

/**
 * The light ramp, used by the calendar and its legend. Five steps, because a
 * sixth is not distinguishable — and it stops well short of white: half a year
 * of bright cells is a texture, not a reading.
 */
const RAMP = [0.07, 0.15, 0.26, 0.4, 0.58];

/** A bar's brightness is its share, floored so the smallest one is still there. */
function barShade(share: number) {
  return `rgba(255,246,235,${(0.18 + 0.6 * share).toFixed(3)})`;
}

const reducedMotion =
  typeof matchMedia === "function" &&
  matchMedia("(prefers-reduced-motion: reduce)").matches;

function count(n: number) {
  return n.toLocaleString();
}

/** "1 day", "62 days" — the s that a template string always gets wrong. */
function plural(n: number, word: string) {
  return `${count(n)} ${word}${n === 1 ? "" : "s"}`;
}

/** Minutes as something a person reads: "3h 20m", "45m", "under a minute". */
function duration(minutes: number) {
  if (minutes < 1) return "under a minute";
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

/** "code.exe" is what the matcher stores; "Code" is what you called it. */
function appName(key: string) {
  const bare = key.replace(/\.(exe|app)$/i, "").split(/[./]/).pop() ?? key;
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

const LANGUAGE_NAMES =
  typeof Intl !== "undefined" && "DisplayNames" in Intl
    ? new Intl.DisplayNames(undefined, { type: "language" })
    : null;

/** Provider keys are internal; these are the names they go by. */
const PROVIDER_NAMES: Record<string, string> = {
  local: "On this machine",
  openai: "OpenAI",
  groq: "Groq",
  deepgram: "Deepgram",
  mistral: "Mistral",
  elevenlabs: "ElevenLabs",
  assemblyai: "AssemblyAI",
  speechmatics: "Speechmatics",
  azure: "Azure",
  google: "Google",
};

function providerName(key: string) {
  return PROVIDER_NAMES[key] ?? key;
}

function languageName(code: string) {
  try {
    return LANGUAGE_NAMES?.of(code) ?? code;
  } catch {
    return code;
  }
}

/* ---- Pieces --------------------------------------------------------------- */

/** The unit of this page: one fact, framed. */
function Card({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return <section className={`glass rounded-xl p-4 ${className}`}>{children}</section>;
}

function Figure({
  value,
  suffix,
  label,
}: {
  value: string;
  suffix?: string;
  label: string;
}) {
  return (
    <div>
      <div className="tabular flex items-baseline gap-1">
        <span className="text-[31px] font-semibold leading-none tracking-[-0.03em] text-[var(--ink)]">
          {value}
        </span>
        {suffix && (
          <span className="text-[14px] font-medium text-[var(--ink-muted)]">{suffix}</span>
        )}
      </div>
      <div className="mt-1.5 text-[13px] leading-snug text-[var(--ink-muted)]">{label}</div>
    </div>
  );
}

/**
 * Speaking speed against typing speed.
 *
 * A comparison, not a score: the tick is where a keyboard sits, and the sweep
 * is where you sit. Echo has no cohort to rank anyone against and will not
 * invent one — the honest claim is "this much faster than typing it".
 */
function SpeedArc({ wpm }: { wpm: number }) {
  const R = 44;
  const LEN = Math.PI * R;
  // 200 wpm is past the top of sustained human dictation, so a full sweep
  // reads as "the end of the scale" rather than a target to chase.
  const frac = Math.min(1, wpm / 200);
  const tickAngle = Math.PI * (1 - Math.min(1, TYPING_WPM / 200));

  return (
    <svg viewBox="0 0 108 62" className="h-[62px] w-[108px] shrink-0" aria-hidden>
      <path
        d="M 10 54 A 44 44 0 0 1 98 54"
        fill="none"
        stroke="rgba(255,246,235,0.09)"
        strokeWidth="6"
        strokeLinecap="round"
      />
      <path
        d="M 10 54 A 44 44 0 0 1 98 54"
        fill="none"
        stroke="var(--ink)"
        strokeWidth="6"
        strokeLinecap="round"
        strokeDasharray={LEN}
        strokeDashoffset={LEN * (1 - frac)}
        style={
          reducedMotion
            ? undefined
            : ({
                "--arc-len": `${LEN}px`,
                animation: "arc-draw 0.9s cubic-bezier(0.22, 1, 0.36, 1) both",
              } as React.CSSProperties)
        }
      />
      {/* Where a keyboard would be on the same scale. */}
      <line
        x1={54 + (R - 7) * Math.cos(tickAngle)}
        y1={54 - (R - 7) * Math.sin(tickAngle)}
        x2={54 + (R + 7) * Math.cos(tickAngle)}
        y2={54 - (R + 7) * Math.sin(tickAngle)}
        stroke="var(--ink-faint)"
        strokeWidth="1.5"
      />
    </svg>
  );
}

/** One row of a breakdown: what it was, how much of it, and the share as a bar. */
function BarRow({
  label,
  value,
  share,
}: {
  label: string;
  value: string;
  share: number;
}) {
  return (
    <div className="flex items-center gap-3">
      <span
        className="w-[96px] shrink-0 truncate text-[13.5px] text-[var(--ink)]"
        title={label}
      >
        {label}
      </span>
      <span className="h-[18px] min-w-0 flex-1 overflow-hidden rounded-[5px] bg-[var(--surface-1)]">
        <span
          className="block h-full rounded-[5px]"
          style={{ width: `${Math.max(2, share * 100)}%`, background: barShade(share) }}
        />
      </span>
      <span className="tabular w-[86px] shrink-0 text-right text-[13px] text-[var(--ink-muted)]">
        {value}
      </span>
    </div>
  );
}

function Breakdown({
  rows,
  empty,
  name = (k: string) => k,
}: {
  rows: Tally[];
  empty: string;
  name?: (key: string) => string;
}) {
  if (rows.length === 0) {
    return <p className="text-[13px] leading-relaxed text-[var(--ink-faint)]">{empty}</p>;
  }
  const total = rows.reduce((sum, r) => sum + r.transcripts, 0) || 1;
  return (
    <div className="space-y-1.5">
      {rows.slice(0, 6).map((row) => (
        <BarRow
          key={row.key}
          label={name(row.key)}
          share={row.transcripts / total}
          value={`${Math.round((row.transcripts / total) * 100)}% · ${count(row.words)}`}
        />
      ))}
    </div>
  );
}

const WEEKS = 26;
const DAY_MS = 86_400_000;

function isoDate(d: Date) {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}

/**
 * Half a year of days, brightest where you said the most.
 *
 * Built from the days that exist rather than from a dense series: a day with
 * nothing in it is simply absent from the data, and drawing it as an empty
 * cell is the whole point of a calendar like this.
 */
function Calendar({ daily }: { daily: DayWords[] }) {
  const byDate = new Map(daily.map((d) => [d.date, d.words]));
  const busiest = Math.max(1, ...daily.map((d) => d.words));

  // Midday, so a daylight-saving shift cannot roll a cell into the day before.
  const today = new Date();
  today.setHours(12, 0, 0, 0);
  // Count back in whole weeks from the Sunday that opens *this* week, so the
  // last column is the week you are in. Subtracting a flat 26×7 days and then
  // snapping back to Sunday ends the calendar up to six days before today —
  // which drops the days most worth seeing.
  const start = new Date(today);
  start.setDate(start.getDate() - start.getDay() - (WEEKS - 1) * 7);

  const columns = Array.from({ length: WEEKS }, (_, w) =>
    Array.from({ length: 7 }, (_, d) => {
      const date = new Date(start.getTime() + (w * 7 + d) * DAY_MS);
      const key = isoDate(date);
      return { date, key, words: byDate.get(key) ?? 0, future: date > today };
    })
  );

  const shade = (words: number) => {
    if (words === 0) return "var(--surface-1)";
    const i = Math.min(RAMP.length - 1, Math.floor((words / busiest) * RAMP.length));
    return `rgba(255,246,235,${RAMP[i]})`;
  };

  /** A month is labelled on the first column that lands in it. */
  const monthLabel = (week: { date: Date }[], i: number) => {
    const first = week[0].date;
    const previous = columns[i - 1]?.[0].date;
    if (previous && previous.getMonth() === first.getMonth()) return null;
    return first.toLocaleDateString(undefined, { month: "short" });
  };

  // Sunday-first rows, labelled on alternate lines: seven labels would be a
  // wall of text next to a chart made of 10px squares.
  const DAY_LABELS = ["", "Mon", "", "Wed", "", "Fri", ""];

  return (
    <div className="flex gap-1.5 overflow-x-auto">
      <div className="flex shrink-0 flex-col gap-[2px] pr-0.5 pt-3">
        {DAY_LABELS.map((label, i) => (
          <span
            key={i}
            className="h-[10px] text-[8.5px] leading-[10px] text-[var(--ink-faint)]"
          >
            {label}
          </span>
        ))}
      </div>
      {/* Right padding so the last month's label has somewhere to sit: the
          labels are wider than the 10px column they start in and would
          otherwise be clipped at the card's edge. */}
      <div className="flex gap-[2px]">
        {columns.map((week, i) => (
          <div key={week[0].key} className="flex flex-col gap-[2px]">
            <span
              className={
                "h-3 whitespace-nowrap text-[9px] leading-3 text-[var(--ink-faint)] " +
                // A label starts in a 10px column and runs wider than it. That
                // is fine everywhere except the final column, where the card's
                // edge would cut it in half — so the last one hangs left.
                (i === columns.length - 1 ? "-translate-x-3.5" : "")
              }
            >
              {monthLabel(week, i)}
            </span>
            {week.map((day) => (
              <span
                key={day.key}
                title={
                  day.words
                    ? `${day.date.toLocaleDateString()} — ${count(day.words)} words`
                    : day.date.toLocaleDateString()
                }
                className="h-[10px] w-[10px] rounded-[2px]"
                style={{
                  background: shade(day.words),
                  // Days that have not happened yet are drawn fainter than an
                  // empty past day, so the calendar does not read as a run of
                  // missed days at the end.
                  opacity: day.future ? 0.35 : 1,
                }}
              />
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

/** When in the day you talk to your computer. */
function HourStrip({ hours }: { hours: number[] }) {
  const busiest = Math.max(1, ...hours);
  return (
    <div>
      <div className="flex h-12 items-end gap-[3px]">
        {hours.map((n, h) => (
          <span
            key={h}
            title={`${String(h).padStart(2, "0")}:00 — ${count(n)} ${
              n === 1 ? "dictation" : "dictations"
            }`}
            className="min-w-0 flex-1 rounded-[3px]"
            style={{
              height: `${Math.max(3, (n / busiest) * 100)}%`,
              background: n
                ? `rgba(255,246,235,${0.14 + 0.66 * (n / busiest)})`
                : "var(--surface-1)",
            }}
          />
        ))}
      </div>
      <div className="mt-1.5 flex justify-between text-[9.5px] text-[var(--ink-faint)]">
        <span>midnight</span>
        <span>midday</span>
        <span>midnight</span>
      </div>
    </div>
  );
}

/* ---- Page ----------------------------------------------------------------- */

export function InsightsPanel() {
  const { data, isLoading } = useQuery({
    queryKey: ["insights"],
    queryFn: commands.getInsights,
  });

  if (isLoading || !data) {
    return (
      <Page title="Insights" width={880}>
        <p className="py-5 text-[13px] text-[var(--ink-muted)]">Counting…</p>
      </Page>
    );
  }

  if (data.transcripts === 0) {
    return (
      <Page
        title="Insights"
        description="What your dictation adds up to — speed, fixes, and where the words went."
        width={880}
      >
        <p className="max-w-[56ch] py-5 text-[13.5px] leading-relaxed text-[var(--ink-muted)]">
          Nothing counted yet. These numbers are worked out from your History, so
          they stay empty while History is switched off — there is nothing stored
          to count. Dictate something with History on and this fills in.
        </p>
      </Page>
    );
  }

  const spokenMinutes = data.spoken_ms / 60_000;
  const wpm = spokenMinutes > 0.05 ? Math.round(data.timed_words / spokenMinutes) : null;
  const fixes = data.dictionary_fixes + data.cleanup_fixes;
  const minutesSaved = Math.max(0, data.words / TYPING_WPM - spokenMinutes);

  const onDevice = data.providers.find((p) => p.key === "local")?.transcripts ?? 0;
  const offlineShare = Math.round((onDevice / data.transcripts) * 100);

  const since = data.since
    ? new Date(data.since).toLocaleDateString(undefined, {
        year: "numeric",
        month: "long",
        day: "numeric",
      })
    : null;

  return (
    <Page
      title="Insights"
      description="Counted from your History, on this machine. None of it is sent anywhere, and clearing History clears it."
      width={880}
    >
      <Group>
        <div className="grid gap-2.5 sm:grid-cols-3">
          <Card className="flex items-center justify-between gap-2">
            <Figure
              value={wpm ? count(wpm) : "—"}
              suffix={wpm ? "wpm" : undefined}
              label={
                wpm
                  ? `spoken — ${(wpm / TYPING_WPM).toFixed(1)}× typing speed`
                  : "speaking speed — measured from your next dictation onward"
              }
            />
            {wpm !== null && <SpeedArc wpm={wpm} />}
          </Card>

          <Card>
            <Figure value={count(fixes)} label="words Echo changed for you" />
            <div className="mt-3 space-y-1 border-t border-[var(--hairline)] pt-2.5 text-[13px] text-[var(--ink-muted)]">
              <div className="flex justify-between gap-3">
                <span>your dictionary</span>
                <span className="tabular text-[var(--ink)]">
                  {count(data.dictionary_fixes)}
                </span>
              </div>
              <div className="flex justify-between gap-3">
                <span>clean-up and punctuation</span>
                <span className="tabular text-[var(--ink)]">{count(data.cleanup_fixes)}</span>
              </div>
            </div>
          </Card>

          <Card>
            <Figure value={count(data.words)} label="words dictated in total" />
            <div className="mt-3 space-y-1 border-t border-[var(--hairline)] pt-2.5 text-[13px] text-[var(--ink-muted)]">
              <div className="flex justify-between gap-3">
                <span>this week</span>
                <span className="tabular text-[var(--ink)]">
                  {count(data.words_last_7_days)}
                </span>
              </div>
              <div className="flex justify-between gap-3">
                <span>times you spoke</span>
                <span className="tabular text-[var(--ink)]">{count(data.transcripts)}</span>
              </div>
            </div>
          </Card>
        </div>

        <p className="max-w-[70ch] text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
          Saying it took {duration(spokenMinutes)}. Typing the same words at{" "}
          {TYPING_WPM} a minute would have taken roughly{" "}
          <strong className="font-medium text-[var(--ink-muted)]">
            {duration(minutesSaved)}
          </strong>{" "}
          longer — an estimate, and a generous one: it counts none of the time you
          spent correcting a transcript.
          {since && ` Counting from ${since}, as far back as your History goes.`}
        </p>
      </Group>

      <Group>
        <div className="grid gap-2.5 lg:grid-cols-2">
          <Card>
            <h4 className="mb-3 text-[14px] font-medium text-[var(--ink)]">
              Apps you dictate into
            </h4>
            <Breakdown
              rows={data.apps}
              name={appName}
              empty="No app recorded yet. Echo stores the focused app alongside each transcript — where it cannot see which app that is, the dictation is counted everywhere else but not here."
            />
          </Card>

          <Card>
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <h4 className="text-[14px] font-medium text-[var(--ink)]">
                {data.streak > 0 ? `${count(data.streak)}-day streak` : "No streak running"}
              </h4>
              <span className="text-[12.5px] text-[var(--ink-faint)]">
                longest {plural(data.longest_streak, "day")} · {plural(data.days, "day")} used
              </span>
            </div>
            <Calendar daily={data.daily} />
            <div className="mt-2.5 flex items-center justify-end gap-1.5 text-[9.5px] text-[var(--ink-faint)]">
              <span>less</span>
              {RAMP.map((o) => (
                <span
                  key={o}
                  className="h-[10px] w-[10px] rounded-[2px]"
                  style={{ background: `rgba(255,246,235,${o})` }}
                />
              ))}
              <span>more</span>
            </div>
          </Card>
        </div>
      </Group>

      <Group>
        <div className="grid gap-2.5 lg:grid-cols-2">
          <Card>
            <div className="mb-3 flex items-baseline justify-between gap-3">
              <h4 className="text-[14px] font-medium text-[var(--ink)]">On device or cloud</h4>
              <span className="tabular text-[12.5px] text-[var(--ink-faint)]">
                {offlineShare}% never left this machine
              </span>
            </div>
            <Breakdown
              rows={data.providers}
              name={providerName}
              empty="Nothing transcribed yet."
            />
          </Card>

          <Card>
            <h4 className="mb-3 text-[14px] font-medium text-[var(--ink)]">Languages</h4>
            <Breakdown
              rows={data.languages}
              name={languageName}
              empty="No language recorded — a decoder reports one only when it is sure."
            />
          </Card>
        </div>
      </Group>

      <Group>
        <Card>
          <h4 className="mb-3 text-[14px] font-medium text-[var(--ink)]">When you dictate</h4>
          <HourStrip hours={data.hours} />
        </Card>
      </Group>
    </Page>
  );
}
