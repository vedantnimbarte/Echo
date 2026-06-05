import { useQuery, useMutation } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

export function SettingsPanel() {
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });

  const { data: historyEnabled } = useQuery({
    queryKey: ["setting", "history_enabled"],
    queryFn: () => commands.getSetting("history_enabled"),
  });

  const setProviderMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("asr_provider", v),
  });

  const setHistoryMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("history_enabled", v),
  });

  return (
    <div className="p-6 space-y-6">
      <h2 className="text-lg font-semibold text-zinc-100">Settings</h2>

      <div className="space-y-4">
        {/* ASR Provider */}
        <label className="block space-y-1">
          <span className="text-sm text-zinc-400">ASR Provider</span>
          <select
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-100 focus:outline-none focus:ring-2 focus:ring-violet-500"
            value={provider ?? "none"}
            onChange={(e) => setProviderMutation.mutate(e.target.value)}
          >
            <option value="none">None (no transcription)</option>
            <option value="openai">OpenAI Whisper API</option>
            <option value="groq">Groq</option>
            <option value="deepgram">Deepgram</option>
          </select>
        </label>

        {/* History toggle */}
        <label className="flex items-center gap-3 cursor-pointer">
          <input
            type="checkbox"
            className="w-4 h-4 accent-violet-500"
            checked={historyEnabled !== "false"}
            onChange={(e) =>
              setHistoryMutation.mutate(e.target.checked ? "true" : "false")
            }
          />
          <span className="text-sm text-zinc-300">Save transcription history</span>
        </label>
      </div>
    </div>
  );
}
