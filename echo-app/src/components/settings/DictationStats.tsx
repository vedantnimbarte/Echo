import { useQuery } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

/**
 * What dictation has added up to.
 *
 * The numbers come out of History rather than a counter of their own — History
 * already is the record, and a second tally would be one more thing to keep in
 * step with it. The visible cost is that turning History off empties this, and
 * the panel says so rather than showing zeroes that look like a bug.
 */

/** Words per minute, speaking. Conservative; conversational speech is faster. */
const SPEAKING_WPM = 130;

/** Words per minute, typing. A competent touch-typist composing prose. */
const TYPING_WPM = 45;

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="glass rounded-lg px-3 py-2.5">
      <div className="text-[17px] font-semibold tracking-tight text-[var(--ink)]">
        {value}
      </div>
      <div className="mt-0.5 text-[10.5px] leading-snug text-[var(--ink-faint)]">
        {label}
      </div>
    </div>
  );
}

/** Thousands separators, and nothing clever beyond that. */
function count(n: number) {
  return n.toLocaleString();
}

/** Minutes as something a person reads: "3h 20m", "45m", "under a minute". */
function duration(minutes: number) {
  if (minutes < 1) return "under a minute";
  const h = Math.floor(minutes / 60);
  const m = Math.round(minutes % 60);
  return h > 0 ? `${h}h ${m}m` : `${m}m`;
}

export function DictationStats() {
  const { data, isLoading } = useQuery({
    queryKey: ["dictation-stats"],
    queryFn: commands.getDictationStats,
  });

  if (isLoading || !data) {
    return <p className="text-[11px] text-[var(--ink-muted)]">Counting…</p>;
  }

  if (data.transcripts === 0) {
    return (
      <p className="max-w-[56ch] text-[11px] leading-relaxed text-[var(--ink-muted)]">
        Nothing counted yet. These numbers are worked out from your History, so
        they stay empty while History is switched off — there is nothing stored
        to count.
      </p>
    );
  }

  // The difference between saying it and typing it. Presented as an estimate
  // because that is what it is: it assumes you would have typed the same words,
  // and it ignores the time spent fixing what the decoder got wrong.
  const minutesSaved =
    data.words / TYPING_WPM - data.words / SPEAKING_WPM;

  const since = data.since
    ? new Date(data.since).toLocaleDateString(undefined, {
        year: "numeric",
        month: "long",
        day: "numeric",
      })
    : null;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Stat value={count(data.words)} label="words dictated" />
        <Stat value={count(data.transcripts)} label="times you spoke" />
        <Stat value={count(data.words_last_7_days)} label="words this week" />
        <Stat value={count(data.days)} label="days used" />
      </div>

      <p className="max-w-[56ch] text-[10.5px] leading-relaxed text-[var(--ink-faint)]">
        Roughly <strong className="font-medium text-[var(--ink-muted)]">
          {duration(minutesSaved)}
        </strong>{" "}
        less than typing the same words — assuming {TYPING_WPM} words a minute
        typed against {SPEAKING_WPM} spoken. It is an estimate, and a generous
        one: it counts none of the time you spent correcting a transcript.
        {since && ` Counting from ${since}, as far back as your History goes.`}
      </p>
    </div>
  );
}
