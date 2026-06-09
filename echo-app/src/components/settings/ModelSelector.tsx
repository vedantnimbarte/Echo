import { useEffect, useState } from "react";
import { Download, Check, Loader2 } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * Lists local Whisper models with download status, lets the user download one
 * (with a live progress bar) and select it as the active local ASR provider.
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
    await commands.setAsrProvider(name);
    queryClient.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
  }

  return (
    <div className="space-y-2">
      <span className="text-sm text-zinc-400">Local Whisper models</span>
      <div className="space-y-2">
        {models.map((m) => {
          const downloading = m.name in progress;
          const isActive = activeProvider === m.name;
          return (
            <div
              key={m.name}
              className="flex items-center justify-between rounded-lg border border-zinc-700 bg-zinc-800/50 px-3 py-2"
            >
              <div className="flex flex-col">
                <span className="text-sm text-zinc-100 capitalize">{m.name}</span>
                <span className="text-xs text-zinc-500">{m.size_mb} MB</span>
              </div>

              {downloading ? (
                <div className="flex items-center gap-2 text-xs text-zinc-400">
                  <Loader2 className="w-4 h-4 animate-spin" />
                  {Math.round((progress[m.name] ?? 0) * 100)}%
                </div>
              ) : m.downloaded ? (
                <button
                  onClick={() => select(m.name)}
                  disabled={isActive}
                  className="flex items-center gap-1 rounded-md px-3 py-1 text-xs transition-colors disabled:opacity-60 bg-violet-600 hover:bg-violet-700 text-white"
                >
                  {isActive ? (
                    <>
                      <Check className="w-3.5 h-3.5" /> Active
                    </>
                  ) : (
                    "Use"
                  )}
                </button>
              ) : (
                <button
                  onClick={() => download(m.name)}
                  className="flex items-center gap-1 rounded-md border border-zinc-600 px-3 py-1 text-xs text-zinc-200 hover:bg-zinc-700"
                >
                  <Download className="w-3.5 h-3.5" /> Download
                </button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
