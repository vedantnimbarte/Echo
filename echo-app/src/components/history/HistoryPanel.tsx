import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2, Copy, CornerDownLeft, Search, Check, BookPlus, Download, Pencil } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { commands, type TranscriptionRecord } from "../../ipc/commands";
import { Page, Group } from "../common/Page";
import { prettyHotkey } from "../common/HotkeyCapture";

/* ---- time helpers --------------------------------------------------------- */

/** Parse a SQLite "YYYY-MM-DD HH:MM:SS" (UTC) timestamp into a Date. */
function parseTs(raw: string): Date {
  // Normalise to ISO and assume UTC when no zone is present.
  const iso = raw.includes("T") ? raw : raw.replace(" ", "T");
  const withZone = /[zZ]|[+-]\d\d:?\d\d$/.test(iso) ? iso : `${iso}Z`;
  return new Date(withZone);
}

function relativeTime(d: Date): string {
  const sec = Math.round((Date.now() - d.getTime()) / 1000);
  if (sec < 45) return "just now";
  if (sec < 90) return "1 min ago";
  const min = Math.round(sec / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr} hr ago`;
  return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function dayLabel(d: Date): string {
  const today = new Date();
  const start = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diffDays = Math.round((start(today) - start(d)) / 86_400_000);
  if (diffDays <= 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays < 7) return d.toLocaleDateString([], { weekday: "long" });
  return d.toLocaleDateString([], { month: "short", day: "numeric", year: "numeric" });
}

function wordCount(text: string): number {
  return text.trim() ? text.trim().split(/\s+/).length : 0;
}

/* ---- greeting -------------------------------------------------------------- */

/**
 * The page's heading, in place of the word "History".
 *
 * The sidebar already says which page you are on, so spending the largest type
 * on that label again says nothing. This says the one thing the window cannot
 * do for you — which keys start dictation, and that they work outside this
 * window, which is the whole point of Echo and the part people miss.
 *
 * The sentence names the action rather than describing a mood: "press these to
 * dictate", not "get back into the flow with these". It reads the same on the
 * first run as on the thousandth, which the earlier wording did not — there is
 * no flow to get back into when you have never dictated. The name is a courtesy
 * on top and the line is written to work without one, because the OS often has
 * nothing worth using.
 *
 * Falls back to the page's name while the shortcut is still being read, and if
 * none is ever set — an empty heading would leave the page with nothing to be
 * found by. The name is the sidebar's, "History" — it said "Dictation" until the
 * button was renamed, and a heading that disagrees with the button you just
 * pressed reads as having landed somewhere else.
 */
function Greeting() {
  const { data: name } = useQuery({
    queryKey: ["account-name"],
    queryFn: commands.accountName,
  });
  const { data: hotkey } = useQuery({ queryKey: ["hotkey"], queryFn: commands.getHotkey });

  // Set below the heading size the other pages use: a sentence this long at
  // full title scale reads as shouting, and it is a greeting, not a banner.
  return (
    <span className="flex flex-wrap items-baseline gap-x-2 gap-y-1 text-[22px]">
      {!hotkey ? (
        "History"
      ) : (
        <>
          {name ? `Hey ${name}, press` : "Press"}
          {/* The keys keep the interface font — a keycap set in Garamond stops
              looking like a key. */}
          <span
            className="flex items-baseline gap-1.5"
            style={{ fontFamily: "var(--font-ui)" }}
          >
            {prettyHotkey(hotkey).map((k, i) => (
              <kbd
                key={`${k}-${i}`}
                className="rounded-md border border-[var(--hairline)] bg-[var(--surface-2)] px-1.5 py-0.5 text-[15px] font-medium tracking-tight text-[var(--ink)]"
              >
                {k}
              </kbd>
            ))}
          </span>
          anywhere to dictate
        </>
      )}
    </span>
  );
}

/* ---- summary --------------------------------------------------------------- */

/** Round numbers worth arriving at, for the bar under the Insights link. */
const MILESTONES = [1_000, 5_000, 10_000, 25_000, 50_000, 100_000, 250_000, 500_000, 1_000_000];

function Stat({ value, label }: { value: string; label: string }) {
  return (
    <div className="tabular flex items-baseline gap-1.5">
      <span className="text-[26px] font-semibold leading-none tracking-[-0.03em] text-[var(--ink)]">
        {value}
      </span>
      <span className="text-[13px] text-[var(--ink-muted)]">{label}</span>
    </div>
  );
}

/**
 * The three figures worth knowing beside your transcripts, and a way into the
 * page that has the rest.
 *
 * Deliberately not a second Insights: the same query backs both, so this shows
 * the three that answer "how am I doing" and hands the other twenty to the page
 * built for them. The bar measures progress to the next round number — it gates
 * nothing, and the link works from the first word.
 */
function Summary({ onOpenInsights }: { onOpenInsights: () => void }) {
  const { data } = useQuery({ queryKey: ["insights"], queryFn: commands.getInsights });
  if (!data) return null;

  const spokenMinutes = data.spoken_ms / 60_000;
  const wpm = spokenMinutes > 0.05 ? Math.round(data.timed_words / spokenMinutes) : null;
  const target = MILESTONES.find((m) => m > data.words) ?? data.words;
  const share = target > 0 ? Math.min(1, data.words / target) : 0;

  return (
    <aside className="glass h-fit rounded-xl">
      <div className="space-y-4 p-5">
        {data.transcripts === 0 ? (
          <p className="text-[13px] leading-relaxed text-[var(--ink-muted)]">
            Nothing counted yet. Dictate something and this fills in.
          </p>
        ) : (
          <>
            <Stat value={data.words.toLocaleString()} label="total words" />
            <Stat value={wpm ? wpm.toLocaleString() : "—"} label="wpm" />
            <Stat value={data.streak.toLocaleString()} label="day streak" />
          </>
        )}
      </div>

      <button
        onClick={onOpenInsights}
        className="w-full space-y-2 border-t border-[var(--hairline)] p-5 text-left transition-colors hover:bg-[var(--surface-2)]"
      >
        <span className="block text-[14px] font-medium text-[var(--ink)]">Your Insights</span>
        <span className="block text-[13px] leading-snug text-[var(--ink-muted)]">
          See how you use your voice.
        </span>
        <span className="flex items-center gap-2.5 pt-1">
          <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-[var(--surface-2)]">
            <span
              className="block h-full rounded-full bg-[var(--ink-muted)]"
              style={{ width: `${share * 100}%` }}
            />
          </span>
          <span className="tabular shrink-0 text-[12.5px] text-[var(--ink-faint)]">
            {data.words.toLocaleString()} / {target.toLocaleString()}
          </span>
        </span>
      </button>
    </aside>
  );
}

/* ---- row ------------------------------------------------------------------ */

function HistoryRow({ record }: { record: TranscriptionRecord }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(record.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1400);
    } catch {
      /* clipboard unavailable — ignore */
    }
  }

  const [fixing, setFixing] = useState(false);
  const [editing, setEditing] = useState(false);
  const when = parseTs(record.created_at);

  // Prefill with the whole transcript when it's short enough to be the mistake
  // itself; otherwise leave it blank rather than making the user delete a
  // paragraph to get at one word.
  const suggestion = record.text.trim().split(/\s+/).length <= 4 ? record.text.trim() : "";

  return (
    <li className="group rounded-xl glass px-3.5 py-2.5 transition hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-2)]">
      <p className="text-[15px] leading-snug text-[var(--ink)]">{record.text}</p>
      <div className="mt-1.5 flex items-center justify-between">
        <p className="text-[12.5px] tracking-tight text-[var(--ink-faint)]">
          {record.provider}
          {record.language ? ` · ${record.language}` : ""} · {wordCount(record.text)} words ·{" "}
          {relativeTime(when)}
        </p>
        <div className="flex items-center gap-1 opacity-0 transition group-hover:opacity-100">
          <button
            onClick={copy}
            aria-label="Copy"
            title="Copy"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-[var(--ink)]" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </button>
          <button
            onClick={() => void commands.injectText(record.text)}
            aria-label="Insert into focused app"
            title="Insert into focused app"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          >
            <CornerDownLeft className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => {
              setEditing((e) => !e);
              setFixing(false);
            }}
            aria-label="Fix this transcript"
            title="Fix this transcript"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          >
            <Pencil className="h-3.5 w-3.5" />
          </button>
          <button
            onClick={() => {
              setFixing((f) => !f);
              setEditing(false);
            }}
            aria-label="Teach Echo a correction"
            title="Teach Echo a correction"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          >
            <BookPlus className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {editing && (
        <TranscriptFix
          original={record.text}
          onDone={() => setEditing(false)}
        />
      )}

      {fixing && (
        <CorrectionForm
          suggestion={suggestion}
          onDone={() => setFixing(false)}
        />
      )}
    </li>
  );
}

/**
 * Turn a misheard phrase into a dictionary entry, by naming both halves.
 *
 * The exact form still has its place — it is how you add a rule for something
 * you have not dictated yet — but [`TranscriptFix`] is the easier path when the
 * mistake is already on screen.
 */
function CorrectionForm({
  suggestion,
  onDone,
}: {
  suggestion: string;
  onDone: () => void;
}) {
  const qc = useQueryClient();
  const [phrase, setPhrase] = useState(suggestion);
  const [replacement, setReplacement] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function submit() {
    const from = phrase.trim();
    const to = replacement.trim();
    if (!from || !to) return;
    try {
      await commands.addDictionaryEntry(from, to);
      qc.invalidateQueries({ queryKey: ["dictionary"] });
      onDone();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="mt-2 space-y-1.5 border-t border-[var(--hairline)] pt-2">
      <p className="text-[12.5px] text-[var(--ink-faint)]">
        Replace what Echo heard with what you meant. Applies to every future
        transcript.
      </p>
      <div className="flex items-center gap-1.5">
        <input
          className="field flex-1 text-[14px]"
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
          placeholder="Echo heard…"
        />
        <span className="shrink-0 text-[var(--ink-faint)]">→</span>
        <input
          className="field flex-1 text-[14px]"
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
          placeholder="You meant…"
          onKeyDown={(e) => e.key === "Enter" && void submit()}
        />
        <button
          onClick={submit}
          disabled={!phrase.trim() || !replacement.trim()}
          className="btn-primary shrink-0 px-2.5 py-1 text-[13px]"
        >
          Save
        </button>
      </div>
      {error && <p className="text-[13px] font-medium text-[var(--ink)]">{error}</p>}
    </div>
  );
}

/* ---- panel ---------------------------------------------------------------- */

/** `onOpenInsights` is App's page state — the summary card links into it. */
export function HistoryPanel({ onOpenInsights }: { onOpenInsights: () => void }) {
  const qc = useQueryClient();
  const [query, setQuery] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["history"],
    queryFn: () => commands.getHistory(200),
  });
  // `?? []` rather than a destructuring default: that only fires on undefined,
  // so a backend answering null reached the grouping loop as null and took the
  // whole window down with it.
  const records = data ?? [];

  const clearMutation = useMutation({
    mutationFn: () => commands.clearHistory(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["history"] }),
  });

  const filtered = query.trim()
    ? records.filter((r) => r.text.toLowerCase().includes(query.toLowerCase()))
    : records;

  // Group consecutive records under their day label (records arrive newest-first).
  const groups: { label: string; items: TranscriptionRecord[] }[] = [];
  for (const r of filtered) {
    const label = dayLabel(parseTs(r.created_at));
    const last = groups[groups.length - 1];
    if (last && last.label === label) last.items.push(r);
    else groups.push({ label, items: [r] });
  }

  return (
    <Page
      title={<Greeting />}
      // Wider than the settings pages: this one carries a column beside its
      // content, the same room Insights takes for the same reason.
      width={920}
    >
      <Group>
        {/* The list is the page; the column beside it is a glance. `lg:` rather
            than always, because below that width two columns would squeeze the
            transcripts — the thing you came here to read — into a gutter. */}
        <div className="grid grid-cols-1 gap-8 lg:grid-cols-[minmax(0,1fr)_248px]">
      <div className="min-w-0 space-y-4">
        <div className="relative">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-faint)]" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcripts…"
          className="field py-1.5 pl-8 pr-2.5"
        />
      </div>

      {isLoading ? (
        <p className="text-[15px] text-[var(--ink-muted)]">Loading…</p>
      ) : filtered.length === 0 ? (
        <p className="text-[15px] text-[var(--ink-muted)]">
          {query.trim() ? "No matching transcripts." : "No history yet."}
        </p>
      ) : (
        <div className="space-y-4">
          {groups.map((g) => (
            <section key={g.label} className="space-y-2">
              <h3 className="px-0.5 text-[13.5px] font-medium text-[var(--ink-muted)]">
                {g.label}
              </h3>
              <ul className="space-y-2">
                {g.items.map((r, i) => (
                  <HistoryRow key={r.id ?? `${g.label}-${i}`} record={r} />
                ))}
              </ul>
            </section>
          ))}
        </div>
      )}
      </div>
          {/* The column, top to bottom: what your dictation adds up to, then
              what you can do to the whole of it. Both are about the record as a
              body rather than any one transcript, which is why they sit away
              from the list and not in the page header over it — and why they
              stay put while the list goes by. A running total that scrolls off
              at the fourth transcript is a total of nothing in particular.

              Pinned only in the two-column layout: stacked, this sits *under*
              the list, and pinning it there would hold the buttons over the
              transcripts you were reading. `self-start` is what makes it
              possible at all — a grid item stretches to the row by default, so
              it would be as tall as the list and have no room to slide.

              ponytail: 90px clears the pinned page header, measured at 89.5 —
              its own 48 + 8 of padding plus one line of greeting. Hard-coded
              because reading it back would mean measuring another component on
              every resize; wrong only if the greeting wraps, which needs a long
              name in a window already too narrow for this column to exist. */}
          <div className="space-y-3 lg:sticky lg:top-[90px] lg:self-start">
            <Summary onOpenInsights={onOpenInsights} />
            <div className="flex gap-2">
              <button
                onClick={async () => {
                  const path = await save({
                    defaultPath: "echo-history.json",
                    filters: [{ name: "JSON", extensions: ["json"] }],
                  });
                  if (path) await commands.exportHistory(path);
                }}
                disabled={records.length === 0}
                className="btn-ghost flex-1 px-2.5 py-1.5 text-[13px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
              >
                <Download className="h-3.5 w-3.5" />
                Export
              </button>
              <button
                onClick={() => clearMutation.mutate()}
                disabled={records.length === 0 || clearMutation.isPending}
                className="btn-ghost flex-1 px-2.5 py-1.5 text-[13px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
              >
                <Trash2 className="h-3.5 w-3.5" />
                Clear all
              </button>
            </div>
          </div>
        </div>
      </Group>
    </Page>
  );
}

/**
 * Fix a transcript by rewriting it, and let Echo work out what changed.
 *
 * Echo still refuses to watch what you type in other applications — that is the
 * thing it exists not to do, and no amount of convenience is worth it. But an
 * edit you make *here*, to a transcript already in front of you, was handed to
 * us deliberately. Diffing that is not surveillance, and it is far less work
 * than typing out both halves of a correction by hand.
 *
 * Only confident, small corrections survive the diff, so most edits teach
 * nothing at all — which is why this reports what it learned rather than
 * claiming success.
 */
function TranscriptFix({
  original,
  onDone,
}: {
  original: string;
  onDone: () => void;
}) {
  const qc = useQueryClient();
  const [text, setText] = useState(original);
  const [learned, setLearned] = useState<{ phrase: string; replacement: string }[] | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  async function save() {
    if (text.trim() === original.trim()) {
      onDone();
      return;
    }
    try {
      const result = await commands.learnFromCorrection(original, text);
      setLearned(result);
      if (result.length > 0) qc.invalidateQueries({ queryKey: ["dictionary"] });
      // Nothing learned is a normal outcome, so close rather than sit there
      // looking like it failed.
      if (result.length === 0) onDone();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="mt-2 space-y-1.5 border-t border-[var(--hairline)] pt-2">
      <p className="text-[12.5px] text-[var(--ink-faint)]">
        Correct the text. If the change looks like a fixed mishearing, Echo adds
        it to your dictionary so it stops happening.
      </p>
      <textarea
        className="field w-full resize-y text-[14px] leading-snug"
        rows={3}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <div className="flex items-center justify-end gap-1.5">
        <button onClick={onDone} className="btn-ghost px-2.5 py-1 text-[13px]">
          Cancel
        </button>
        <button
          onClick={save}
          disabled={!text.trim()}
          className="btn-primary shrink-0 px-2.5 py-1 text-[13px]"
        >
          Save
        </button>
      </div>
      {learned && learned.length > 0 && (
        <div className="space-y-1 pt-0.5">
          <p className="text-[12.5px] text-[var(--ink-muted)]">Learned:</p>
          {learned.map((l) => (
            <p key={l.phrase} className="text-[13px] text-[var(--ink)]">
              {l.phrase} <span className="text-[var(--ink-faint)]">&rarr;</span>{" "}
              {l.replacement}
            </p>
          ))}
        </div>
      )}
      {error && <p className="text-[13px] text-[var(--ink)]">{error}</p>}
    </div>
  );
}
