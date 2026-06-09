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
        <h2 className="text-lg font-semibold text-zinc-100">Custom Dictionary</h2>
        <div className="flex gap-2">
          <button
            onClick={handleImport}
            className="flex items-center gap-1 rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 transition-colors"
          >
            <Upload className="w-3.5 h-3.5" /> Import
          </button>
          <button
            onClick={handleExport}
            className="flex items-center gap-1 rounded-lg border border-zinc-700 px-3 py-1.5 text-xs text-zinc-300 hover:bg-zinc-800 transition-colors"
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
          className="flex-1 bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus:outline-none focus:ring-2 focus:ring-violet-500"
          placeholder="Phrase (e.g. router file)"
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
        />
        <input
          className="flex-1 bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm text-zinc-100 placeholder:text-zinc-500 focus:outline-none focus:ring-2 focus:ring-violet-500"
          placeholder="Replacement (e.g. src/agents/router.rs)"
          value={replacement}
          onChange={(e) => setReplacement(e.target.value)}
        />
        <button
          type="submit"
          disabled={!phrase || !replacement || addMutation.isPending}
          className="bg-violet-600 hover:bg-violet-700 disabled:opacity-50 px-4 py-2 rounded-lg text-sm text-white flex items-center gap-1 transition-colors"
        >
          <Plus className="w-4 h-4" />
          Add
        </button>
      </form>

      {/* Entries list */}
      {isLoading ? (
        <p className="text-zinc-500 text-sm">Loading…</p>
      ) : entries.length === 0 ? (
        <p className="text-zinc-500 text-sm">No entries yet.</p>
      ) : (
        <ul className="space-y-2">
          {entries.map((entry) => (
            <li
              key={entry.id}
              className="flex items-center gap-3 bg-zinc-800/50 border border-zinc-700 rounded-lg px-4 py-2 text-sm"
            >
              <input
                type="checkbox"
                className="w-4 h-4 accent-violet-500"
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
                    ? "text-zinc-300 font-mono"
                    : "text-zinc-500 font-mono line-through"
                }
              >
                {entry.phrase}
              </span>
              <span className="text-zinc-600">→</span>
              <span
                className={
                  entry.enabled
                    ? "text-zinc-400 font-mono flex-1"
                    : "text-zinc-600 font-mono flex-1 line-through"
                }
              >
                {entry.replacement}
              </span>
              <button
                onClick={() => entry.id != null && deleteMutation.mutate(entry.id)}
                className="text-zinc-600 hover:text-red-400 transition-colors"
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
