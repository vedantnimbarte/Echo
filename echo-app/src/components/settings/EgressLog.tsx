import { useState } from "react";
import { Globe, ShieldCheck, Trash2 } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

function relative(iso: string): string {
  // SQLite writes naive UTC; mark it so the browser doesn't read it as local.
  const then = new Date(iso.replace(" ", "T") + "Z").getTime();
  const secs = Math.max(0, Math.round((Date.now() - then) / 1000));
  if (secs < 60) return `${secs}s ago`;
  if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
  if (secs < 86400) return `${Math.round(secs / 3600)}h ago`;
  return `${Math.round(secs / 86400)}d ago`;
}

/**
 * The outbound-request log.
 *
 * The wording here is load-bearing. This shows requests **Echo** made — it is
 * not, and must never be presented as, proof that nothing else left the
 * machine. A process can't observe its own OS's traffic, and a native plugin
 * can call out without going through the code this instruments. Overclaiming
 * would make the feature worse than not having it.
 */
export function EgressLog() {
  const qc = useQueryClient();
  const [open, setOpen] = useState(false);

  const { data: status } = useQuery({
    queryKey: ["egress-status"],
    queryFn: commands.getEgressStatus,
  });
  const { data: log = [] } = useQuery({
    queryKey: ["egress-log"],
    queryFn: () => commands.getEgressLog(100),
    enabled: open,
  });

  return (
    <div className="space-y-3">
      <div className="flex items-start gap-2.5">
        {status?.offline_capable ? (
          <ShieldCheck className="mt-px h-4 w-4 shrink-0 text-[var(--ink)]" />
        ) : (
          <Globe className="mt-px h-4 w-4 shrink-0 text-[var(--ink-muted)]" />
        )}
        <div className="min-w-0 flex-1">
          <p className="text-[12px] font-medium text-[var(--ink)]">
            {status?.offline_capable
              ? "Configured to work offline"
              : "This setup contacts the network"}
          </p>
          <ul className="mt-1 space-y-0.5">
            {(status?.reasons ?? []).map((r) => (
              <li key={r} className="text-[10.5px] leading-snug text-[var(--ink-muted)]">
                · {r}
              </li>
            ))}
          </ul>
        </div>
      </div>

      <div className="flex items-center justify-between gap-2">
        <button
          onClick={() => setOpen((o) => !o)}
          className="btn-ghost px-2.5 py-1 text-[11px]"
        >
          {open ? "Hide" : "Show"} request log
          {status ? ` (${status.recent_count} in 24h)` : ""}
        </button>
        {open && log.length > 0 && (
          <button
            onClick={async () => {
              await commands.clearEgressLog();
              qc.invalidateQueries({ queryKey: ["egress-log"] });
              qc.invalidateQueries({ queryKey: ["egress-status"] });
            }}
            className="btn-ghost px-2.5 py-1 text-[11px] text-[var(--ink-muted)] hover:text-[var(--ink)]"
          >
            <Trash2 className="h-3.5 w-3.5" />
            Clear
          </button>
        )}
      </div>

      {open && (
        <div className="glass max-h-56 overflow-y-auto rounded-lg p-2">
          {log.length === 0 ? (
            <p className="px-1 py-1 text-[11px] text-[var(--ink-muted)]">
              No requests logged.
            </p>
          ) : (
            <ul className="space-y-0.5">
              {log.map((r) => (
                <li
                  key={r.id}
                  className="flex items-baseline gap-2 px-1 py-0.5 text-[11px]"
                >
                  <span className="font-mono text-[var(--ink)]">{r.host}</span>
                  <span className="truncate text-[var(--ink-muted)]">{r.purpose}</span>
                  <span className="ml-auto shrink-0 text-[10px] text-[var(--ink-faint)]">
                    {relative(r.created_at)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
        This lists requests Echo itself made. It is not proof that nothing else
        left your machine — Echo can’t see traffic from other programs, and a
        native plugin can make requests that bypass this log entirely.
      </p>
    </div>
  );
}
