import { useEffect, useState } from "react";
import { Download, Check, Loader2, Trash2, AlertTriangle } from "lucide-react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands, type ModelInfo } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * Local Whisper models: download, choose, and remove.
 *
 * These are the largest files Echo puts on disk — the catalog spans 75 MB to
 * 1.5 GB — so the models are drawn as a shelf of cards rather than a menu. A
 * grid gives the catalog its real shape: two families, English-only and
 * multilingual, each climbing the same size ladder, so the trade you are
 * actually making is visible in one look instead of read row by row.
 *
 * Every card carries a bar along its bottom edge showing what it costs relative
 * to the biggest model in the catalog, and the header totals it. The same bar
 * fills as a model downloads — it is the same quantity arriving.
 *
 * Model choice is stored separately from the provider: choosing one sets the
 * model setting for its engine and switches the active provider to match —
 * `local` for whisper models, `nemo` for the NVIDIA transducer, which also
 * needs its own engine binary installed before it can be picked.
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
  const { data: nemoModel } = useQuery({
    queryKey: ["setting", "nemo_model"],
    queryFn: () => commands.getSetting("nemo_model"),
  });
  const { data: nemo } = useQuery({
    queryKey: ["nemo-status"],
    queryFn: commands.nemoStatus,
  });
  // Fraction of the NeMo engine binary that has arrived, while it is arriving.
  const [enginePct, setEnginePct] = useState<number | null>(null);

  // Map of model name → download progress (0..1). Present only while downloading.
  const [progress, setProgress] = useState<Record<string, number>>({});
  // The model whose Remove button is awaiting a second click.
  const [confirming, setConfirming] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onModelDownloadProgress((name, p) =>
        setProgress((prev) => ({ ...prev, [name]: p }))
      ),
      echoEvents.onNemoEngineProgress((p) => setEnginePct(p)),
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
    const engine = models.find((m) => m.name === name)?.engine ?? "whisper";
    // The transducer runs in its own binary, and selecting a model Echo cannot
    // load would leave a dead engine selected. Fetch it first, on the click
    // that needs it, rather than shipping 100 MB nobody asked for.
    if (engine === "nemo" && !nemo?.engine_installed) {
      setEnginePct(0);
      try {
        await commands.downloadNemoEngine();
      } finally {
        setEnginePct(null);
        queryClient.invalidateQueries({ queryKey: ["nemo-status"] });
      }
    }
    await commands.setWhisperModel(name);
    await commands.setAsrProvider(engine === "nemo" ? "nemo" : "local");
    queryClient.invalidateQueries({ queryKey: ["setting", "whisper_model"] });
    queryClient.invalidateQueries({ queryKey: ["setting", "nemo_model"] });
    queryClient.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
    queryClient.invalidateQueries({ queryKey: ["whisper-ready"] });
    queryClient.invalidateQueries({ queryKey: ["nemo-status"] });
    // The pill reports the engine too, and it is a separate webview.
    void echoEvents.emitEngineChanged();
  }

  // Which model the highlight belongs to depends on which engine is active:
  // the two remember their own choice, so switching back restores it.
  const effectiveModel =
    activeProvider === "nemo" ? nemoModel || "nemotron-streaming-0.6b" : activeModel || "base.en";
  const downloaded = models.filter((m) => m.downloaded);
  const usedMb = downloaded.reduce((sum, m) => sum + m.size_mb, 0);
  const scale = largestSize(models);

  // Split by language family rather than a badge on every card: it is the one
  // thing that changes which model is right for you, and it is the axis the
  // grid can carry for free. Derived from the catalog, so a model added to the
  // backend lands in the right half on its own.
  const families = [
    { label: "English only", models: models.filter((m) => m.english_only) },
    { label: "All languages", models: models.filter((m) => !m.english_only) },
  ].filter((f) => f.models.length > 0);

  const installingEngine = enginePct !== null;

  function card(m: ModelInfo) {
    const downloading = m.name in progress;
    const isActive = effectiveModel === m.name;
    // The model in use can't be removed: doing so would break transcription
    // with nothing on screen explaining why.
    const inUse = isActive && activeProvider === (m.engine === "nemo" ? "nemo" : "local");
    const pendingRemoval = confirming === m.name;
    const removing = deleteMutation.isPending && deleteMutation.variables === m.name;
    // The bar means disk: how much this model costs once it is here, or how
    // much of it has arrived while it is still downloading.
    const fill = downloading
      ? (progress[m.name] ?? 0) * 100
      : m.downloaded
        ? Math.max(4, (m.size_mb / scale) * 100)
        : 0;

    return (
      <div
        key={m.name}
        className={
          "relative overflow-hidden rounded-lg border px-3 py-2.5 transition " +
          (inUse
            ? "border-[var(--hairline-strong)] bg-[var(--surface-2)] shadow-[var(--edge-light)]"
            : "border-[var(--hairline)] bg-[var(--surface-1)]")
        }
      >
        {/* Confirming takes over the card rather than growing it: at this size
            there is no room for a prompt beside the name, and a destructive
            step deserves the whole surface anyway. */}
        {pendingRemoval ? (
          <div className="flex flex-col gap-1.5">
            <span className="text-[13px] text-[var(--ink-muted)]">
              Remove {formatSize(m.size_mb)}?
            </span>
            <span className="flex items-center gap-1.5">
              <button
                onClick={() => deleteMutation.mutate(m.name)}
                disabled={removing}
                className="btn-primary px-2 py-0.5 text-[13px]"
              >
                {removing ? "Removing…" : "Remove"}
              </button>
              <button
                onClick={() => setConfirming(null)}
                className="btn-ghost px-2 py-0.5 text-[13px]"
              >
                Keep
              </button>
            </span>
          </div>
        ) : (
          <div className="flex items-center justify-between gap-2">
            <span className="flex min-w-0 flex-col">
              <span className="truncate text-[14px] font-medium text-[var(--ink)]">{m.name}</span>
              <span className="tabular text-[12.5px] text-[var(--ink-faint)]">
                {formatSize(m.size_mb)}
                {m.downloaded && " on disk"}
                {/* The transducer is a different engine, not a bigger whisper:
                    worth saying on the card, because it punctuates itself and
                    needs its own binary. */}
                {m.engine === "nemo" &&
                  (nemo?.engine_installed
                    ? " · NVIDIA engine"
                    : ` · NVIDIA engine, +${nemo?.engine_mb ?? 0} MB`)}
              </span>
            </span>

            <span className="flex shrink-0 items-center gap-1">
              {downloading ? (
                <span className="flex items-center gap-1.5 text-[13px] text-[var(--ink-muted)]">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  <span className="tabular">{Math.round((progress[m.name] ?? 0) * 100)}%</span>
                </span>
              ) : !m.downloaded ? (
                <button
                  onClick={() => download(m.name)}
                  title={`Download ${m.name} (${formatSize(m.size_mb)})`}
                  aria-label={`Download ${m.name}`}
                  className="btn-ghost p-1.5"
                >
                  <Download className="h-3.5 w-3.5" />
                </button>
              ) : (
                <>
                  {inUse ? (
                    // A state, not a control: the card is already lit, and a
                    // button you cannot press is just something else to read.
                    <Check className="h-3.5 w-3.5 text-[var(--ink)]" aria-label="In use" />
                  ) : installingEngine && m.engine === "nemo" ? (
                    <span className="flex items-center gap-1.5 text-[13px] text-[var(--ink-muted)]">
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      <span className="tabular">{Math.round((enginePct ?? 0) * 100)}%</span>
                    </span>
                  ) : (
                    <button
                      onClick={() => select(m.name)}
                      className="btn-primary px-2 py-0.5 text-[13px]"
                    >
                      Use
                    </button>
                  )}
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
                    className="btn-ghost p-1.5 text-[var(--ink-muted)] hover:text-[var(--ink)] disabled:opacity-35"
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </>
              )}
            </span>
          </div>
        )}

        {fill > 0 && (
          <span
            aria-hidden
            className="absolute bottom-0 left-0 h-[3px] rounded-r-full transition-[width] duration-300 ease-out motion-reduce:transition-none"
            style={{ width: `${fill}%`, background: "rgba(255,246,235,0.32)" }}
          />
        )}
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <p className="text-[13px] text-[var(--ink-muted)]">
        {downloaded.length === 0 ? (
          "Nothing downloaded yet."
        ) : (
          <>
            {downloaded.length} downloaded, taking{" "}
            <span className="tabular text-[var(--ink)]">{formatSize(usedMb)}</span> on disk
          </>
        )}
      </p>

      {families.map((family) => (
        <div key={family.label} className="space-y-2">
          <h4 className="text-[13px] font-medium text-[var(--ink-faint)]">{family.label}</h4>
          {/* Three across: the English family is exactly one row of the size
              ladder, and medium — the outlier at 1.5 GB — ends up alone, which
              is what it is. */}
          <div className="grid grid-cols-3 gap-2">{family.models.map(card)}</div>
        </div>
      ))}

      {deleteMutation.isError && (
        <span className="flex items-start gap-1.5 text-[13px] font-medium leading-snug text-[var(--ink)]">
          <AlertTriangle className="mt-px h-3 w-3 shrink-0" />
          {String(deleteMutation.error)}
        </span>
      )}
    </div>
  );
}
