import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2 } from "lucide-react";
import { commands } from "../../ipc/commands";

export function HistoryPanel() {
  const qc = useQueryClient();

  const { data: records = [], isLoading } = useQuery({
    queryKey: ["history"],
    queryFn: () => commands.getHistory(50),
  });

  const clearMutation = useMutation({
    mutationFn: () => commands.clearHistory(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["history"] }),
  });

  return (
    <div className="p-6 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-zinc-100">History</h2>
        <button
          onClick={() => clearMutation.mutate()}
          disabled={records.length === 0 || clearMutation.isPending}
          className="flex items-center gap-1 text-sm text-zinc-500 hover:text-red-400 disabled:opacity-40 transition-colors"
        >
          <Trash2 className="w-3.5 h-3.5" />
          Clear all
        </button>
      </div>

      {isLoading ? (
        <p className="text-zinc-500 text-sm">Loading…</p>
      ) : records.length === 0 ? (
        <p className="text-zinc-500 text-sm">No history yet.</p>
      ) : (
        <ul className="space-y-2">
          {records.map((r) => (
            <li
              key={r.id}
              className="bg-zinc-800/50 border border-zinc-700 rounded-lg px-4 py-3 space-y-1"
            >
              <p className="text-zinc-100 text-sm">{r.text}</p>
              <p className="text-zinc-600 text-xs">
                {r.provider}
                {r.language ? ` · ${r.language}` : ""} · {r.created_at}
              </p>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
