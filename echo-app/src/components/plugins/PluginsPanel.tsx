import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2, Puzzle, ShieldAlert } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, type PluginManifest } from "../../ipc/commands";

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

  // Inspect before installing: the user has to see what the plugin claims and
  // what a plugin can do before any code is loaded. The backend refuses an
  // install that isn't acknowledged, so this isn't cosmetic.
  const [pending, setPending] = useState<
    { path: string; manifest: PluginManifest } | null
  >(null);
  const [error, setError] = useState<string | null>(null);

  async function handleInstall() {
    setError(null);
    const selected = await open({
      multiple: false,
      filters: [{ name: "Plugin library", extensions: ["dll", "dylib", "so"] }],
    });
    if (typeof selected !== "string") return;
    try {
      const manifest = await commands.inspectPlugin(selected);
      setPending({ path: selected, manifest });
    } catch (e) {
      setError(String(e));
    }
  }

  async function confirmInstall() {
    if (!pending) return;
    try {
      await commands.installPlugin(pending.path, true);
      invalidate();
      setPending(null);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-[var(--ink)]">Plugins</h2>
        <button
          onClick={handleInstall}
          className="btn-primary px-3 py-1.5 text-xs"
        >
          <Puzzle className="w-3.5 h-3.5" /> Install from file
        </button>
      </div>

      <div className="glass flex items-start gap-2.5 rounded-lg px-3 py-2.5">
        <ShieldAlert className="mt-px h-4 w-4 shrink-0 text-[var(--ink)]" />
        <p className="text-[11px] leading-snug text-[var(--ink-muted)]">
          <span className="font-medium text-[var(--ink)]">
            Plugins are not sandboxed.
          </span>{" "}
          A plugin runs inside Echo with your account's privileges: it can read
          your files, transcripts and microphone, make network requests this
          app's request log can't see, and read stored API keys. The permission
          list in a manifest is advisory and is <em>not</em> enforced. Only
          install plugins you built or whose source you have read.
        </p>
      </div>

      {error && (
        <p className="text-[11px] font-medium text-[var(--ink)]">{error}</p>
      )}

      {pending && (
        <div className="glass space-y-2.5 rounded-lg p-3">
          <p className="text-[12px] font-medium text-[var(--ink)]">
            Install “{pending.manifest.name}” v{pending.manifest.version}?
          </p>
          {pending.manifest.author && (
            <p className="text-[11px] text-[var(--ink-muted)]">
              by {pending.manifest.author}
            </p>
          )}
          <p className="text-[11px] text-[var(--ink-muted)]">
            Declares:{" "}
            {pending.manifest.permissions.length > 0
              ? pending.manifest.permissions.join(", ")
              : "no permissions"}{" "}
            <span className="text-[var(--ink-faint)]">
              — advisory only; the plugin is not restricted to this.
            </span>
          </p>
          <div className="flex gap-1.5">
            <button onClick={confirmInstall} className="btn-primary px-3 py-1.5 text-xs">
              Install anyway
            </button>
            <button
              onClick={() => setPending(null)}
              className="btn-ghost px-3 py-1.5 text-xs"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {isLoading ? (
        <p className="text-[var(--ink-muted)] text-sm">Loading…</p>
      ) : plugins.length === 0 ? (
        <p className="text-[var(--ink-muted)] text-sm">No plugins installed.</p>
      ) : (
        <ul className="space-y-2">
          {plugins.map((p) => (
            <li
              key={p.name}
              className="flex items-center gap-3 glass rounded-lg px-4 py-2 text-sm"
            >
              <input
                type="checkbox"
                className="h-4 w-4 accent-white"
                checked={p.enabled}
                onChange={(e) =>
                  toggleMutation.mutate({ name: p.name, enabled: e.target.checked })
                }
                aria-label={p.enabled ? "Disable plugin" : "Enable plugin"}
              />
              <div className="flex flex-col flex-1">
                <span className="text-[var(--ink)]">
                  {p.name}{" "}
                  <span className="text-[var(--ink-muted)] text-xs">v{p.version}</span>
                </span>
                {p.description && (
                  <span className="text-xs text-[var(--ink-muted)]">{p.description}</span>
                )}
                {p.author && (
                  <span className="text-xs text-[var(--ink-faint)]">by {p.author}</span>
                )}
                {p.permissions.length > 0 && (
                  <span className="text-xs text-[var(--ink-faint)]">
                    declares: {p.permissions.join(", ")}
                  </span>
                )}
              </div>
              <button
                onClick={() => uninstallMutation.mutate(p.name)}
                className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
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
