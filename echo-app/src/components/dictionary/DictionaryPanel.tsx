import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Trash2, Plus, Download, Upload } from "lucide-react";
import { save, open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";

export function DictionaryPanel() {
  const qc = useQueryClient();
  const [phrase, setPhrase] = useState("");
  const [replacement, setReplacement] = useState("");

  const { data: entries = [], isLoading } = useQuery({
    queryKey: ["dictionary"],
    queryFn: () => commands.listDictionary(),
  });

  const addMutation = useMutation({
    mutationFn: () => commands.addDictionaryEntry(phrase, replacement),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["dictionary"] });
      setPhrase("");
      setReplacement("");
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (id: number) => commands.deleteDictionaryEntry(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dictionary"] }),
  });

  const toggleMutation = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) =>
      commands.toggleDictionaryEntry(id, enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["dictionary"] }),
  });

  async function handleExport() {
    const path = await save({
      defaultPath: "echo-dictionary.json",
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (path) await commands.exportDictionary(path);
  }

  async function handleImport() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (typeof selected === "string") {
      await commands.importDictionary(selected);
      qc.invalidateQueries({ queryKey: ["dictionary"] });
    }
  }

  return (
    <div className="p-6 space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-[var(--ink)]">Custom Dictionary</h2>
        <div className="flex gap-2">
          <button
            onClick={handleImport}
            className="btn-ghost px-3 py-1.5 text-xs"
          >
            <Upload className="w-3.5 h-3.5" /> Import
          </button>
          <button
            onClick={handleExport}
            className="btn-ghost px-3 py-1.5 text-xs"
          >
            <Download className="w-3.5 h-3.5" /> Export
          </button>
        </div>
      </div>

      {/* Add entry form */}
      <form
        className="flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          if (phrase && replacement) addMutation.mutate();
        }}
      >
        <input
          className="field flex-1 px-3 py-2 text-sm"
          placeholder="Phrase (e.g. router file)"
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
        />
        <input
          className="field flex-1 px-3 py-2 text-sm"
          placeholder="Replacement (e.g. src/agents/router.rs)"
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
        />
        <button
          type="submit"
          disabled={!phrase || !replacement || addMutation.isPending}
          className="btn-primary px-4 py-2 text-sm"
        >
          <Plus className="w-4 h-4" />
          Add
        </button>
      </form>

      {/* Entries list */}
      {isLoading ? (
        <p className="text-[var(--ink-muted)] text-sm">Loading…</p>
      ) : entries.length === 0 ? (
        <p className="text-[var(--ink-muted)] text-sm">No entries yet.</p>
      ) : (
        <ul className="space-y-2">
          {entries.map((entry) => (
            <li
              key={entry.id}
              className="flex items-center gap-3 glass rounded-lg px-4 py-2 text-sm"
            >
              <input
                type="checkbox"
                className="h-4 w-4 accent-white"
                checked={entry.enabled}
                disabled={entry.id == null}
                onChange={(e) =>
                  entry.id != null &&
                  toggleMutation.mutate({ id: entry.id, enabled: e.target.checked })
                }
                aria-label={entry.enabled ? "Disable entry" : "Enable entry"}
              />
              <span
                className={
                  entry.enabled
                    ? "text-[var(--ink)] font-mono"
                    : "text-[var(--ink-muted)] font-mono line-through"
                }
              >
                {entry.phrase}
              </span>
              <span className="text-[var(--ink-faint)]">→</span>
              <span
                className={
                  entry.enabled
                    ? "text-[var(--ink-muted)] font-mono flex-1"
                    : "text-[var(--ink-faint)] font-mono flex-1 line-through"
                }
              >
                {entry.replacement}
              </span>
              <button
                onClick={() => entry.id != null && deleteMutation.mutate(entry.id)}
                className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
                aria-label="Delete entry"
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
