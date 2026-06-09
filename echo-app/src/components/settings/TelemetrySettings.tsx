import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { commands } from "../../ipc/commands";

/**
 * Telemetry controls: opt-in toggle, a breakdown of locally stored event
 * counts, and a delete button. All telemetry stays on-device.
 */
export function TelemetrySettings() {
  const qc = useQueryClient();
  const [showData, setShowData] = useState(false);

  const { data: enabled } = useQuery({
    queryKey: ["setting", "telemetry_enabled"],
    queryFn: () => commands.getSetting("telemetry_enabled"),
  });

  const { data: summary = [] } = useQuery({
    queryKey: ["telemetry-summary"],
    queryFn: commands.getTelemetrySummary,
    enabled: showData,
  });

  const setEnabledMutation = useMutation({
    mutationFn: (v: boolean) => commands.setTelemetryEnabled(v),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["setting", "telemetry_enabled"] }),
  });

  const clearMutation = useMutation({
    mutationFn: () => commands.clearTelemetry(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["telemetry-summary"] }),
  });

  return (
    <div className="space-y-3 border-t border-zinc-800 pt-4">
      <span className="text-sm text-zinc-400">Usage data (local only)</span>

      <label className="flex items-center gap-3 cursor-pointer">
        <input
          type="checkbox"
          className="w-4 h-4 accent-violet-500"
          checked={enabled !== "false"}
          onChange={(e) => setEnabledMutation.mutate(e.target.checked)}
        />
        <span className="text-sm text-zinc-300">
          Collect anonymous usage data on this device
        </span>
      </label>

      <button
        onClick={() => setShowData((v) => !v)}
        className="text-xs text-violet-400 hover:underline"
      >
        {showData ? "Hide collected data" : "View collected data"}
      </button>

      {showData && (
        <div className="rounded-lg border border-zinc-700 bg-zinc-800/50 p-3 space-y-1">
          {summary.length === 0 ? (
            <p className="text-xs text-zinc-500">No events recorded.</p>
          ) : (
            summary.map((s) => (
              <div
                key={s.event_type}
                className="flex justify-between text-xs text-zinc-300"
              >
                <span className="font-mono">{s.event_type}</span>
                <span className="text-zinc-400">{s.count}</span>
              </div>
            ))
          )}
          <button
            onClick={() => clearMutation.mutate()}
            className="mt-2 rounded-md border border-zinc-600 px-3 py-1 text-xs text-red-400 hover:bg-zinc-700"
          >
            Delete all telemetry data
          </button>
        </div>
      )}
    </div>
  );
}
