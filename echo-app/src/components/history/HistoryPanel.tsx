import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2, Copy, CornerDownLeft, Search, Check } from "lucide-react";
import { commands, type TranscriptionRecord } from "../../ipc/commands";

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

  const when = parseTs(record.created_at);

  return (
    <li className="group rounded-xl border border-white/8 bg-white/[0.025] px-3.5 py-2.5 transition hover:border-white/14 hover:bg-white/[0.04]">
      <p className="text-[13px] leading-snug text-[var(--ink)]">{record.text}</p>
      <div className="mt-1.5 flex items-center justify-between">
        <p className="text-[10.5px] tracking-tight text-[var(--ink-faint)]">
          {record.provider}
          {record.language ? ` · ${record.language}` : ""} · {wordCount(record.text)} words ·{" "}
          {relativeTime(when)}
        </p>
        <div className="flex items-center gap-1 opacity-0 transition group-hover:opacity-100">
          <button
            onClick={copy}
            aria-label="Copy"
            title="Copy"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-white/8 hover:text-[var(--ink)]"
          >
            {copied ? (
              <Check className="h-3.5 w-3.5 text-emerald-400" />
            ) : (
              <Copy className="h-3.5 w-3.5" />
            )}
          </button>
          <button
            onClick={() => void commands.injectText(record.text)}
            aria-label="Insert into focused app"
            title="Insert into focused app"
            className="flex h-6 w-6 items-center justify-center rounded-md text-[var(--ink-muted)] transition hover:bg-white/8 hover:text-[var(--aurora-1)]"
          >
            <CornerDownLeft className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </li>
  );
}

/* ---- panel ---------------------------------------------------------------- */

export function HistoryPanel() {
  const qc = useQueryClient();
  const [query, setQuery] = useState("");

  const { data: records = [], isLoading } = useQuery({
    queryKey: ["history"],
    queryFn: () => commands.getHistory(200),
  });

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
    <div className="mx-auto max-w-[560px] space-y-4 p-5">
      <div className="flex items-center justify-between">
        <h2 className="text-[15px] font-semibold tracking-tight">History</h2>
        <button
          onClick={() => clearMutation.mutate()}
          disabled={records.length === 0 || clearMutation.isPending}
          className="flex items-center gap-1.5 rounded-lg border border-white/10 px-2.5 py-1 text-[11px] text-[var(--ink-muted)] transition hover:border-rose-500/50 hover:bg-rose-500/10 hover:text-rose-300 disabled:opacity-40"
        >
          <Trash2 className="h-3.5 w-3.5" />
          Clear all
        </button>
      </div>

      <div className="relative">
        <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--ink-faint)]" />
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search transcripts…"
          className="w-full rounded-lg border border-white/10 bg-white/5 py-1.5 pl-8 pr-2.5 text-[13px] text-[var(--ink)] outline-none transition focus:border-[var(--aurora-2)]/60 focus:bg-white/8"
        />
      </div>

      {isLoading ? (
        <p className="text-[13px] text-[var(--ink-muted)]">Loading…</p>
      ) : filtered.length === 0 ? (
        <p className="text-[13px] text-[var(--ink-muted)]">
          {query.trim() ? "No matching transcripts." : "No history yet."}
        </p>
      ) : (
        <div className="space-y-4">
          {groups.map((g) => (
            <section key={g.label} className="space-y-2">
              <h3 className="px-0.5 text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
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
  );
}
