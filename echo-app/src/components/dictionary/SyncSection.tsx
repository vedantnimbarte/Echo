import { useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { FolderOpen, RefreshCw } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { Group } from "../common/Page";

const FOLDER = "dictionary_sync_folder";
const ENABLED = "dictionary_sync_enabled";

/**
 * Sync the dictionary through a folder the user already syncs.
 *
 * Its own component, not more of DictionaryPanel, so the panel stays about the
 * entries and this stays about where they are copied to. The merge itself is
 * all on the Rust side (core::dictionary::sync); this only chooses the folder,
 * flips the switch, and reports what the last sync did.
 *
 * The warning about encryption is on the page, not behind a hint: it is the one
 * thing here someone has to know *before* choosing a folder, not after.
 */
export function SyncSection() {
  const qc = useQueryClient();

  const { data: folder } = useQuery({
    queryKey: ["setting", FOLDER],
    queryFn: () => commands.getSetting(FOLDER),
  });
  const { data: enabledRaw } = useQuery({
    queryKey: ["setting", ENABLED],
    queryFn: () => commands.getSetting(ENABLED),
  });
  const enabled = enabledRaw === "true";

  const { data: status } = useQuery({
    queryKey: ["dictionary-sync"],
    queryFn: commands.getDictionarySyncStatus,
  });

  // A sync that ran in the background may have changed the entries above, and
  // it certainly changed the status line.
  useEffect(() => {
    const unlisten = echoEvents.onDictionarySynced(() => {
      qc.invalidateQueries({ queryKey: ["dictionary-sync"] });
      qc.invalidateQueries({ queryKey: ["dictionary"] });
      qc.invalidateQueries({ queryKey: ["dict-profiles"] });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [qc]);

  const syncNow = useMutation({
    mutationFn: commands.syncDictionaryNow,
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ["dictionary-sync"] });
      qc.invalidateQueries({ queryKey: ["dictionary"] });
      qc.invalidateQueries({ queryKey: ["dict-profiles"] });
    },
  });

  async function save(key: string, value: string) {
    await commands.setSetting(key, value);
    await qc.invalidateQueries({ queryKey: ["setting", key] });
    syncNow.mutate();
  }

  async function chooseFolder() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") await save(FOLDER, selected);
  }

  return (
    <Group
      title="Sync"
      hint="Echo merges each entry rather than overwriting the file, so changes made on two computers while they were offline both survive. If the same entry was changed on both, the later change wins. Deleted entries stay deleted everywhere."
    >
      <p className="text-[13.5px] leading-relaxed text-[var(--ink-muted)]">
        Keep this dictionary the same on every computer by choosing a folder you
        already sync, such as Dropbox, OneDrive, iCloud Drive, Syncthing or a
        network share. Echo keeps a file called echo-dictionary.json there.{" "}
        <strong className="font-medium text-[var(--ink)]">
          The file is not encrypted: anyone who can read that folder can read
          your dictionary.
        </strong>
      </p>

      <div className="flex items-center gap-2">
        <span
          className="field min-w-0 flex-1 truncate px-3 py-2 font-mono text-[13px]"
          title={folder ?? undefined}
        >
          {folder || "No folder chosen"}
        </span>
        <button onClick={chooseFolder} className="btn-ghost shrink-0 px-3 py-1.5 text-xs">
          <FolderOpen className="h-3.5 w-3.5" /> Choose folder
        </button>
      </div>

      <div className="flex items-center justify-between gap-3">
        <label className="flex items-center gap-2 text-[14px] text-[var(--ink)]">
          <input
            type="checkbox"
            className="h-4 w-4 accent-white"
            checked={enabled}
            disabled={!folder}
            onChange={(e) => save(ENABLED, e.target.checked ? "true" : "false")}
          />
          Sync the dictionary through this folder
        </label>
        <button
          onClick={() => syncNow.mutate()}
          disabled={!enabled || !folder || syncNow.isPending}
          className="btn-ghost shrink-0 px-3 py-1.5 text-xs"
        >
          <RefreshCw className={"h-3.5 w-3.5" + (syncNow.isPending ? " animate-spin" : "")} />
          Sync now
        </button>
      </div>

      <div className="space-y-1 text-[13px] text-[var(--ink-muted)]" aria-live="polite">
        <p>
          Last synced:{" "}
          {status?.last_synced_at
            ? new Date(status.last_synced_at).toLocaleString()
            : "never"}
        </p>
        {status?.last_error && (
          <p role="alert" className="text-[var(--danger,#e5484d)]">
            {status.last_error}
          </p>
        )}
        {(status?.conflict_copies.length ?? 0) > 0 && (
          <p>
            Merged in {status!.conflict_copies.length} conflicted{" "}
            {status!.conflict_copies.length === 1 ? "copy" : "copies"} your sync
            service made: {status!.conflict_copies.join(", ")}. Their entries are
            now in echo-dictionary.json, so you can delete them.
          </p>
        )}
      </div>
    </Group>
  );
}
