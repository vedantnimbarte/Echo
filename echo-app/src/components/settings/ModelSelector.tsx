import { useEffect, useState } from "react";
import { Download, Check, Loader2 } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * Lists local Whisper models with download status and lets the user download
 * one (live progress) and select it as the active local model. Model choice is
 * stored separately from the provider: selecting one sets `whisper_model` and
 * switches the active provider to `local`.
 */
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

  return (
    <div className="space-y-1.5">
      <span className="text-[11px] font-medium uppercase tracking-wide text-[var(--ink-faint)]">
        Local model
      </span>
      <div className="space-y-1.5">
        {models.map((m) => {
          const downloading = m.name in progress;
          const isActive = activeProvider === "local" && effectiveModel === m.name;
          return (
            <div
              key={m.name}
              className="flex items-center justify-between rounded-lg border border-white/8 bg-white/[0.025] px-3 py-2"
            >
              <div className="flex min-w-0 flex-col">
                <span className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--ink)]">
                  {m.name}
                  <span className="rounded bg-white/8 px-1 py-px text-[9px] uppercase tracking-wide text-[var(--ink-muted)]">
                    {m.english_only ? "EN" : "multi"}
                  </span>
                </span>
                <span className="text-[10.5px] text-[var(--ink-faint)]">{m.size_mb} MB</span>
              </div>

              {downloading ? (
                <div className="flex items-center gap-1.5 text-[11px] text-[var(--ink-muted)]">
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                  {Math.round((progress[m.name] ?? 0) * 100)}%
                </div>
              ) : m.downloaded ? (
                <button
                  onClick={() => select(m.name)}
                  disabled={isActive}
                  className={
                    "flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-medium transition disabled:opacity-100 " +
                    (isActive
                      ? "bg-[var(--aurora-2)]/20 text-[var(--aurora-1)]"
                      : "bg-[var(--aurora-2)] text-white hover:brightness-110")
                  }
                >
                  {isActive ? (
                    <>
                      <Check className="h-3.5 w-3.5" /> Active
                    </>
                  ) : (
                    "Use"
                  )}
                </button>
              ) : (
                <button
                  onClick={() => download(m.name)}
                  className="flex items-center gap-1 rounded-md border border-white/12 px-2.5 py-1 text-[11px] text-[var(--ink)] transition hover:bg-white/8"
                >
                  <Download className="h-3.5 w-3.5" /> Download
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
