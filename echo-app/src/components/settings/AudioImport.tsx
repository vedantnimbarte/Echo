import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";

/**
 * Transcribe a recording the user already has — a voice memo, a call, an
 * interview — with the same offline engine that handles dictation.
 *
 * The result is shown rather than injected. An import is not aimed at a text
 * cursor the way a dictation is, and pasting a twenty-minute transcript into
 * whatever happened to be focused would be a genuinely bad surprise.
 */
export function AudioImport() {
  const [text, setText] = useState<string | null>(null);
  const [name, setName] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  const run = useMutation({
    mutationFn: async () => {
      const formats = await commands.supportedImportFormats();
      const picked = await open({
        multiple: false,
        filters: [{ name: "Audio", extensions: formats }],
      });
      if (typeof picked !== "string") return null;
      setName(picked.split(/[\\/]/).pop() ?? picked);
      setText(null);
      return commands.transcribeFile(picked);
    },
    onSuccess: (result) => {
      if (result !== null) setText(result);
    },
  });

  async function copy() {
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="space-y-3">
      <button
        type="button"
        className="btn-ghost text-[11px]"
        disabled={run.isPending}
        onClick={() => run.mutate()}
      >
        {run.isPending ? "Transcribing…" : "Choose an audio file…"}
      </button>

      {run.isPending && name && (
        <p className="text-[10.5px] leading-relaxed text-[var(--ink-faint)]">
          Transcribing {name}. Long recordings take a while — this runs entirely
          on your machine.
        </p>
      )}

      {run.error != null && (
        <p className="text-[11px] leading-snug text-[var(--ink)]">
          {String(run.error)}
        </p>
      )}

      {text !== null && (
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <span className="truncate text-[11px] font-medium text-[var(--ink-muted)]">
              {name}
            </span>
            <button type="button" className="btn-ghost text-[11px]" onClick={copy}>
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <textarea
            readOnly
            value={text}
            rows={8}
            className="field w-full resize-y font-mono text-[11px] leading-relaxed"
          />
        </div>
      )}
    </div>
  );
}
