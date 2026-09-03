import { useEffect, useState } from "react";
import { Download, Check, Loader2, Trash2, AlertTriangle } from "lucide-react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands, type ModelInfo } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * Local Whisper models: download, choose, and remove.
 *
 * These are the largest files Echo puts on disk — the catalog spans 75 MB to
 * 1.5 GB — so the list is drawn as a shelf rather than a menu. Each downloaded
 * model carries a bar showing what it costs relative to the biggest one, and
 * the header totals it. That turns "which model?" and "what is this costing
 * me?" into the same glance, and gives Remove something to be measured against.
 *
 * Model choice is stored separately from the provider: choosing one sets
 * `whisper_model` and switches the active provider to `local`.
 */

/** The largest model in the catalog sets the scale every bar is drawn against. */
function largestSize(models: ModelInfo[]): number {
  return models.reduce((max, m) => Math.max(max, m.size_mb), 1);
}

function formatSize(mb: number): string {
  return mb < 1024 ? `${mb} MB` : `${(mb / 1024).toFixed(1)} GB`;
}

export function ModelSelector() {
  const queryClient = useQueryClient();
  const { data: models = [] } = useQuery({
    queryKey: ["asr-models"],
    queryFn: commands.listModels,
  });
  const { data: activeProvider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: activeModel } = useQuery({
    queryKey: ["setting", "whisper_model"],
    queryFn: () => commands.getSetting("whisper_model"),
  });

  // Map of model name → download progress (0..1). Present only while downloading.
  const [progress, setProgress] = useState<Record<string, number>>({});
  // The model whose Remove button is awaiting a second click.
  const [confirming, setConfirming] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onModelDownloadProgress((name, p) =>
        setProgress((prev) => ({ ...prev, [name]: p }))
      ),
      echoEvents.onModelDownloadComplete((name) => {
        setProgress((prev) => {
          const next = { ...prev };
          delete next[name];
          return next;
        });
        queryClient.invalidateQueries({ queryKey: ["asr-models"] });
      }),
    ]);
    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [queryClient]);

  const deleteMutation = useMutation({
    mutationFn: (name: string) => commands.deleteModel(name),
    onSuccess: () => {
      setConfirming(null);
      queryClient.invalidateQueries({ queryKey: ["asr-models"] });
    },
  });

  async function download(name: string) {
    setProgress((prev) => ({ ...prev, [name]: 0 }));
    try {
      await commands.downloadModel(name);
    } catch {
      setProgress((prev) => {
        const next = { ...prev };
        delete next[name];
        return next;
      });
    }
  }

  async function select(name: string) {
    await commands.setWhisperModel(name);
    await commands.setAsrProvider("local");
    queryClient.invalidateQueries({ queryKey: ["setting", "whisper_model"] });
    queryClient.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
  }

  // Default to base.en in the highlight when nothing is explicitly chosen yet.
  const effectiveModel = activeModel || "base.en";
  const downloaded = models.filter((m) => m.downloaded);
  const usedMb = downloaded.reduce((sum, m) => sum + m.size_mb, 0);
  const scale = largestSize(models);

  return (
    <div className="space-y-3">
      <p className="text-[11px] text-[var(--ink-muted)]">
        {downloaded.length === 0 ? (
          "Nothing downloaded yet."
        ) : (
          <>
            {downloaded.length} downloaded ·{" "}
            <span className="tabular text-[var(--ink)]">{formatSize(usedMb)}</span> on disk
          </>
        )}
      </p>

      <div className="space-y-1.5">
        {models.map((m) => {
          const downloading = m.name in progress;
          const isActive = effectiveModel === m.name;
          // The model in use can't be removed: doing so would break
          // transcription with nothing on screen explaining why.
          const inUse = isActive && activeProvider === "local";
          const pendingRemoval = confirming === m.name;
          const removing = deleteMutation.isPending && deleteMutation.variables === m.name;

          return (
            <div
              key={m.name}
              className="relative overflow-hidden rounded-lg glass px-3.5 py-2.5"
            >
              <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-2">
                <div className="flex min-w-0 flex-col gap-0.5">
                  <span className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--ink)]">
                    {m.name}
                    <span className="rounded bg-[var(--surface-2)] px-1 py-px text-[9px] uppercase tracking-wide text-[var(--ink-muted)]">
                      {m.english_only ? "EN" : "multi"}
                    </span>
                  </span>
                  <span className="tabular text-[10.5px] text-[var(--ink-faint)]">
                    {formatSize(m.size_mb)}
                    {m.downloaded && " on disk"}
                  </span>
                </div>

                {downloading ? (
                  <span className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
                    <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    <span className="tabular">
                      {Math.round((progress[m.name] ?? 0) * 100)}%
                    </span>
                  </span>
                ) : pendingRemoval ? (
                  <span className="flex items-center gap-2">
                    <span className="text-[11px] text-[var(--ink-muted)]">
                      Remove {formatSize(m.size_mb)}?
                    </span>
                    <button
                      onClick={() => deleteMutation.mutate(m.name)}
                      disabled={removing}
                      className="btn-primary px-2.5 py-1 text-[11px]"
                    >
                      {removing ? "Removing…" : "Remove"}
                    </button>
                    <button
                      onClick={() => setConfirming(null)}
                      className="btn-ghost px-2.5 py-1 text-[11px]"
                    >
                      Keep
                    </button>
                  </span>
                ) : m.downloaded ? (
                  <span className="flex items-center gap-1.5">
                    <button
                      onClick={() => select(m.name)}
                      disabled={inUse}
                      className={
                        "flex items-center gap-1 rounded-lg px-2.5 py-1 text-[11px] font-medium transition disabled:opacity-100 " +
                        (inUse ? "bg-[var(--surface-3)] text-[var(--ink)]" : "btn-primary")
                      }
                    >
                      {inUse ? (
                        <>
                          <Check className="h-3.5 w-3.5" /> In use
                        </>
                      ) : (
                        "Use"
                      )}
                    </button>
                    <button
                      onClick={() => {
                        deleteMutation.reset();
                        setConfirming(m.name);
                      }}
                      disabled={inUse}
                      title={
                        inUse
                          ? "Echo is using this model. Switch to another one first."
                          : `Remove ${m.name} from this machine`
                      }
                      aria-label={`Remove ${m.name}`}
                      className="btn-ghost px-2 py-1 text-[var(--ink-muted)] hover:text-[var(--ink)] disabled:opacity-35"
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </span>
                ) : (
                  <button
                    onClick={() => download(m.name)}
                    className="btn-ghost gap-1 px-2.5 py-1 text-[11px]"
                  >
                    <Download className="h-3.5 w-3.5" /> Download
                  </button>
                )}
              </div>

              {/* What this model costs, drawn against the largest in the
                  catalog. Only downloaded models get a bar — it measures disk
                  actually spent, which is what Remove gives back. */}
              {m.downloaded && (
                <span
                  aria-hidden
                  className="absolute bottom-0 left-0 h-[3px] rounded-r-full"
                  style={{
                    width: `${Math.max(4, (m.size_mb / scale) * 100)}%`,
                    background: "rgba(255,255,255,0.32)",
                  }}
                />
              )}
            </div>
          );
        })}
      </div>

      {deleteMutation.isError && (
        <span className="flex items-start gap-1.5 text-[11px] font-medium leading-snug text-[var(--ink)]">
          <AlertTriangle className="mt-px h-3 w-3 shrink-0" />
          {String(deleteMutation.error)}
        </span>
      )}
    </div>
  );
}
