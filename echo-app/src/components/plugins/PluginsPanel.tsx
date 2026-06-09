import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2, Puzzle } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";

export function PluginsPanel() {
  const qc = useQueryClient();

  const { data: plugins = [], isLoading } = useQuery({
    queryKey: ["plugins"],
    queryFn: commands.listPlugins,
  });

  const invalidate = () => qc.invalidateQueries({ queryKey: ["plugins"] });

  const toggleMutation = useMutation({
    mutationFn: ({ name, enabled }: { name: string; enabled: boolean }) =>
      enabled ? commands.enablePlugin(name) : commands.disablePlugin(name),
    onSuccess: invalidate,
  });

  const uninstallMutation = useMutation({
    mutationFn: (name: string) => commands.uninstallPlugin(name),
    onSuccess: invalidate,
  });

  async function handleInstall() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Plugin library", extensions: ["dll", "dylib", "so"] }],
    });
    if (typeof selected === "string") {
      await commands.installPlugin(selected);
      invalidate();
    }
  }

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-zinc-100">Plugins</h2>
        <button
          onClick={handleInstall}
          className="flex items-center gap-1 rounded-lg bg-violet-600 hover:bg-violet-700 px-3 py-1.5 text-xs text-white transition-colors"
        >
          <Puzzle className="w-3.5 h-3.5" /> Install from file
        </button>
      </div>

      <p className="text-xs text-zinc-500">
        Plugins run in-process with full trust. Only install plugins you trust.
      </p>

      {isLoading ? (
        <p className="text-zinc-500 text-sm">Loading…</p>
      ) : plugins.length === 0 ? (
        <p className="text-zinc-500 text-sm">No plugins installed.</p>
      ) : (
        <ul className="space-y-2">
          {plugins.map((p) => (
            <li
              key={p.name}
              className="flex items-center gap-3 bg-zinc-800/50 border border-zinc-700 rounded-lg px-4 py-2 text-sm"
            >
              <input
                type="checkbox"
                className="w-4 h-4 accent-violet-500"
                checked={p.enabled}
                onChange={(e) =>
                  toggleMutation.mutate({ name: p.name, enabled: e.target.checked })
                }
                aria-label={p.enabled ? "Disable plugin" : "Enable plugin"}
              />
              <div className="flex flex-col flex-1">
                <span className="text-zinc-200">
                  {p.name}{" "}
                  <span className="text-zinc-500 text-xs">v{p.version}</span>
                </span>
                {p.description && (
                  <span className="text-xs text-zinc-500">{p.description}</span>
                )}
                {p.author && (
                  <span className="text-xs text-zinc-600">by {p.author}</span>
                )}
              </div>
              <button
                onClick={() => uninstallMutation.mutate(p.name)}
                className="text-zinc-600 hover:text-red-400 transition-colors"
                aria-label="Uninstall plugin"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
