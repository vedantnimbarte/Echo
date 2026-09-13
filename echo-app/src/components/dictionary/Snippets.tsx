import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2 } from "lucide-react";
import { commands, type Snippet } from "../../ipc/commands";
import { Group } from "../common/Page";

/**
 * Voice snippets: a trigger phrase and the block of text it stands for.
 *
 * Edits save when a field loses focus rather than behind a Save button. A
 * snippet is two fields, and a button per row would be most of the row.
 */
export function Snippets() {
  const qc = useQueryClient();
  const [trigger, setTrigger] = useState("");
  const [body, setBody] = useState("");
  const [error, setError] = useState<string | null>(null);

  const { data: snippets = [] } = useQuery({
    queryKey: ["snippets"],
    queryFn: commands.listSnippets,
  });

  // The backend refuses a trigger with no words and an empty body, and says
  // why; showing that is better than a click that silently does nothing.
  const save = useMutation({
    mutationFn: (s: Snippet) => commands.saveSnippet(s),
    onMutate: () => setError(null),
    onError: (e) => setError(String(e)),
    onSettled: () => qc.invalidateQueries({ queryKey: ["snippets"] }),
  });

  const remove = useMutation({
    mutationFn: (id: number) => commands.deleteSnippet(id),
    onSettled: () => qc.invalidateQueries({ queryKey: ["snippets"] }),
  });

  const canAdd = trigger.trim() !== "" && body.trim() !== "";

  return (
    <Group
      hint={
        <>
          <p>
            Say the trigger on its own — pause, say it, pause — and Echo inserts
            the text exactly as written here: line breaks, numbers and
            punctuation untouched.
          </p>
          <p className="mt-2">
            Inside a longer sentence the trigger is left as ordinary words, so
            “sign off” can still mean sign off. Capitals and punctuation don't
            matter. Text with more than one line is always pasted, because typing
            a line break presses Return and would send a half-finished message.
          </p>
        </>
      }
    >
      <div className="space-y-6">
        <form
          className="space-y-2"
          onSubmit={(e) => {
            e.preventDefault();
            if (!canAdd) return;
            save.mutate(
              { id: null, trigger, body, enabled: true },
              {
                onSuccess: () => {
                  setTrigger("");
                  setBody("");
                },
              },
            );
          }}
        >
          <input
            className="field w-full px-3 py-2 text-sm"
            placeholder="Trigger (e.g. insert my address)"
            aria-label="Trigger phrase"
            value={trigger}
            onChange={(e) => setTrigger(e.target.value)}
          />
          <textarea
            className="field w-full resize-y px-3 py-2 font-mono text-sm"
            rows={4}
            placeholder="Text to insert"
            aria-label="Snippet text"
            value={body}
            onChange={(e) => setBody(e.target.value)}
          />
          <button
            type="submit"
            disabled={!canAdd || save.isPending}
            className="btn-primary px-4 py-2 text-sm"
          >
            <Plus className="h-4 w-4" />
            Add snippet
          </button>
        </form>

        {error && <p className="text-[13px] font-medium text-[var(--ink)]">{error}</p>}

        {snippets.length === 0 ? (
          <p className="text-sm text-[var(--ink-muted)]">No snippets yet.</p>
        ) : (
          <ul className="space-y-2">
            {snippets.map((s) => (
              <li key={s.id} className="glass space-y-2 rounded-lg px-4 py-3 text-sm">
                <div className="flex items-center gap-3">
                  <input
                    type="checkbox"
                    className="h-4 w-4 accent-white"
                    checked={s.enabled}
                    onChange={(e) => save.mutate({ ...s, enabled: e.target.checked })}
                    aria-label={s.enabled ? "Disable snippet" : "Enable snippet"}
                  />
                  <input
                    className="field flex-1 font-mono text-[13px]"
                    defaultValue={s.trigger}
                    aria-label="Trigger phrase"
                    onBlur={(e) =>
                      e.target.value !== s.trigger &&
                      save.mutate({ ...s, trigger: e.target.value })
                    }
                  />
                  <button
                    onClick={() => s.id != null && remove.mutate(s.id)}
                    className="text-[var(--ink-faint)] transition-colors hover:text-[var(--ink)]"
                    aria-label="Delete snippet"
                  >
                    <Trash2 className="h-4 w-4" />
                  </button>
                </div>
                <textarea
                  className="field w-full resize-y font-mono text-[13px]"
                  rows={Math.min(8, s.body.split("\n").length)}
                  defaultValue={s.body}
                  aria-label="Snippet text"
                  onBlur={(e) =>
                    e.target.value !== s.body && save.mutate({ ...s, body: e.target.value })
                  }
                />
              </li>
            ))}
          </ul>
        )}
      </div>
    </Group>
  );
}
