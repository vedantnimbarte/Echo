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

  /* ---- profiles: named groups an app profile can switch on ---------------- */

  const { data: profiles = [] } = useQuery({
    queryKey: ["dict-profiles"],
    queryFn: commands.listProfiles,
  });
  const [newProfile, setNewProfile] = useState("");

  const invalidateProfiles = () => {
    qc.invalidateQueries({ queryKey: ["dict-profiles"] });
    qc.invalidateQueries({ queryKey: ["dictionary"] });
  };

  const addProfileMutation = useMutation({
    mutationFn: () => commands.addProfile(newProfile),
    onSuccess: () => {
      invalidateProfiles();
      setNewProfile("");
    },
  });

  const deleteProfileMutation = useMutation({
    mutationFn: (id: number) => commands.deleteProfile(id),
    onSuccess: invalidateProfiles,
  });

  const setEntryProfileMutation = useMutation({
    mutationFn: ({ id, profileId }: { id: number; profileId: number | null }) =>
      commands.setDictionaryEntryProfile(id, profileId),
    onSuccess: invalidateProfiles,
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

      {/* Profiles: groups that a per-app profile can switch on */}
      <div className="glass space-y-2 rounded-lg p-3">
        <p className="text-[12px] font-medium text-[var(--ink)]">Profiles</p>
        <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
          Entries with no profile always apply. Put an entry in a profile and it
          only applies while an app using that profile is focused — set that up
          in Settings → Per-app profiles.
        </p>
        <div className="flex gap-1.5">
          <input
            className="field flex-1 text-[12px]"
            placeholder="New profile name"
            value={newProfile}
            onChange={(e) => setNewProfile(e.target.value)}
            onKeyDown={(e) =>
              e.key === "Enter" && newProfile.trim() && addProfileMutation.mutate()
            }
          />
          <button
            onClick={() => addProfileMutation.mutate()}
            disabled={!newProfile.trim()}
            className="btn-primary shrink-0 px-2.5 py-1 text-[11px]"
          >
            <Plus className="h-3.5 w-3.5" />
            Add
          </button>
        </div>
        {profiles.length > 0 && (
          <ul className="flex flex-wrap gap-1.5">
            {profiles.map((p) => (
              <li
                key={p.id}
                className="flex items-center gap-1.5 rounded-md border border-[var(--hairline)] bg-[var(--surface-1)] px-2 py-0.5 text-[11px]"
              >
                {p.name}
                <button
                  onClick={() => p.id != null && deleteProfileMutation.mutate(p.id)}
                  className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
                  aria-label={`Delete profile ${p.name}`}
                  title="Delete profile (its entries become global)"
                >
                  <Trash2 className="h-3 w-3" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

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
              {profiles.length > 0 && (
                <select
                  className="field w-32 shrink-0 text-[11px]"
                  value={entry.profile_id == null ? "global" : String(entry.profile_id)}
                  disabled={entry.id == null}
                  onChange={(e) =>
                    entry.id != null &&
                    setEntryProfileMutation.mutate({
                      id: entry.id,
                      profileId:
                        e.target.value === "global" ? null : Number(e.target.value),
                    })
                  }
                  aria-label="Profile"
                >
                  <option value="global">Always</option>
                  {profiles.map((p) => (
                    <option key={p.id} value={String(p.id)}>
                      {p.name}
                    </option>
                  ))}
                </select>
              )}
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
