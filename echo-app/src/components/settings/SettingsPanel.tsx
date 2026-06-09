import { useState } from "react";
import { useQuery, useMutation } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";
import { ModelSelector } from "./ModelSelector";
import { CloudProviders } from "./CloudProviders";

export function SettingsPanel() {
  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });

  const { data: historyEnabled } = useQuery({
    queryKey: ["setting", "history_enabled"],
    queryFn: () => commands.getSetting("history_enabled"),
  });

  const { data: autoInject } = useQuery({
    queryKey: ["setting", "auto_inject"],
    queryFn: () => commands.getSetting("auto_inject"),
  });

  const { data: injectDelay } = useQuery({
    queryKey: ["setting", "inject_delay_ms"],
    queryFn: () => commands.getSetting("inject_delay_ms"),
  });

  const setAutoInjectMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("auto_inject", v),
  });

  const setInjectDelayMutation = useMutation({
    mutationFn: (v: string) => commands.setSetting("inject_delay_ms", v),
  });

  const [permissionStatus, setPermissionStatus] = useState<boolean | null>(null);
  async function checkPermission() {
    setPermissionStatus(await commands.checkAccessibilityPermission());
  }

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

        {/* Local Whisper models */}
        <ModelSelector />

        {/* Cloud provider API keys */}
        <CloudProviders />

        {/* Text injection */}
        <div className="space-y-3 border-t border-zinc-800 pt-4">
          <span className="text-sm text-zinc-400">Text injection</span>

          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              className="w-4 h-4 accent-violet-500"
              checked={autoInject !== "false"}
              onChange={(e) =>
                setAutoInjectMutation.mutate(e.target.checked ? "true" : "false")
              }
            />
            <span className="text-sm text-zinc-300">
              Inject text into the focused app after transcription
            </span>
          </label>

          <label className="flex items-center gap-3">
            <span className="text-sm text-zinc-300">Inject delay (ms)</span>
            <input
              type="number"
              min={0}
              className="w-24 bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm text-zinc-100 focus:outline-none focus:ring-2 focus:ring-violet-500"
              defaultValue={injectDelay ?? "0"}
              onBlur={(e) => setInjectDelayMutation.mutate(e.target.value || "0")}
            />
          </label>

          <div className="flex items-center gap-3">
            <button
              onClick={checkPermission}
              className="rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 transition-colors"
            >
              Check accessibility permission
            </button>
            {permissionStatus !== null && (
              <span
                className={
                  permissionStatus ? "text-xs text-green-400" : "text-xs text-red-400"
                }
              >
                {permissionStatus ? "Granted" : "Not granted"}
              </span>
            )}
          </div>
          <p className="text-xs text-zinc-500">
            macOS requires Accessibility permission (System Settings → Privacy &
            Security → Accessibility). Linux requires <code>xdotool</code> (X11)
            or <code>ydotool</code> (Wayland).
          </p>
        </div>

        {/* History toggle */}
        <label className="flex items-center gap-3 cursor-pointer border-t border-zinc-800 pt-4">
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
