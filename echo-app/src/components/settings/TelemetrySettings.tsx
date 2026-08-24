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
    <div className="space-y-3 border-t border-[var(--hairline)] pt-4">
      <span className="text-sm text-[var(--ink-muted)]">Usage data (local only)</span>

      <label className="flex items-center gap-3 cursor-pointer">
        <input
          type="checkbox"
          className="h-4 w-4 accent-white"
          checked={enabled !== "false"}
          onChange={(e) => setEnabledMutation.mutate(e.target.checked)}
        />
        <span className="text-sm text-[var(--ink)]">
          Collect anonymous usage data on this device
        </span>
      </label>

      <button
        onClick={() => setShowData((v) => !v)}
        className="text-xs text-[var(--ink-muted)] underline-offset-2 hover:text-[var(--ink)] hover:underline"
      >
        {showData ? "Hide collected data" : "View collected data"}
      </button>

      {showData && (
        <div className="glass rounded-lg p-3 space-y-1">
          {summary.length === 0 ? (
            <p className="text-xs text-[var(--ink-muted)]">No events recorded.</p>
          ) : (
            summary.map((s) => (
              <div
                key={s.event_type}
                className="flex justify-between text-xs text-[var(--ink)]"
              >
                <span className="font-mono">{s.event_type}</span>
                <span className="text-[var(--ink-muted)]">{s.count}</span>
              </div>
            ))
          )}
          <button
            onClick={() => clearMutation.mutate()}
            className="btn-ghost mt-2 px-3 py-1 text-xs"
          >
            Delete all telemetry data
          </button>
        </div>
      )}
    </div>
  );
}
