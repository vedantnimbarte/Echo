import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "../../ipc/commands";

/**
 * Transcribe a recording the user already has — a voice memo, a call, an
 * interview — with the same offline engine that handles dictation.
 *
 * "Label speakers" is the one option that leaves the machine. Whisper cannot
 * tell voices apart, so ticking it sends the whole file to the active cloud
 * engine instead. The box is only enabled when that engine can do it, and the
 * line under it names where the file goes before anyone ticks it.
 *
 * The result is shown rather than injected. An import is not aimed at a text
 * cursor the way a dictation is, and pasting a twenty-minute transcript into
 * whatever happened to be focused would be a genuinely bad surprise.
 */
export function AudioImport() {
  const qc = useQueryClient();
  const [text, setText] = useState<string | null>(null);
  const [name, setName] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [speakers, setSpeakers] = useState(false);

  // The same query keys the engine picker uses, so switching engines there
  // updates this box without a reload.
  const { data: engine } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: providers = [] } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });
  const active = providers.find((p) => p.id === engine);
  const canLabel = active?.speaker_labels === true;
  // Held separately from `speakers` so switching to an engine that cannot do
  // it silently unticks the box rather than leaving a stale choice armed.
  const labelling = speakers && canLabel;
  const able = providers.filter((p) => p.speaker_labels).map((p) => p.label);

  let speakersHint: string;
  if (active && canLabel) {
    speakersHint = `Uploads the whole recording to ${active.label}, which works out who is speaking. Unticked, nothing leaves your machine.`;
  } else {
    const why = active
      ? `${active.label} doesn't label speakers.`
      : engine === "none"
        ? "Transcription is turned off."
        : "The offline Whisper engine doesn't label speakers.";
    speakersHint = `${why} Switch the engine to ${able.join(", ")} to use this — the recording is then uploaded to that provider.`;
  }

  // Audio a crash interrupted before it could be transcribed. Normally empty,
  // which is why this sits above the picker rather than in a group of its own:
  // when there is something here it is the most urgent thing on the page, and
  // the rest of the time it costs no room at all.
  const { data: recovered = [] } = useQuery({
    queryKey: ["recovered-recordings"],
    queryFn: commands.recoveredRecordings,
  });

  function transcribe(path: string, withSpeakers = false) {
    setName(path.split(/[\\/]/).pop() ?? path);
    setText(null);
    return commands.transcribeFile(path, undefined, withSpeakers);
  }

  const run = useMutation({
    mutationFn: async () => {
      const formats = await commands.supportedImportFormats();
      const picked = await open({
        multiple: false,
        filters: [{ name: "Audio", extensions: formats }],
      });
      if (typeof picked !== "string") return null;
      return transcribe(picked, labelling);
    },
    onSuccess: (result) => {
      if (result !== null) setText(result);
    },
  });

  // A rescued dictation is always decoded locally, whatever the box says: it
  // was a dictation, not a meeting, and uploading it would be a surprise.
  const rescue = useMutation({
    mutationFn: (path: string) => transcribe(path),
    onSuccess: (result) => setText(result),
  });

  const discard = useMutation({
    mutationFn: commands.discardRecovered,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["recovered-recordings"] }),
  });

  async function copy() {
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  const busy = run.isPending || rescue.isPending;

  return (
    <div className="space-y-3">
      {recovered.map((path) => (
        <div
          key={path}
          className="glass flex flex-wrap items-center gap-x-3 gap-y-2 rounded-lg px-3 py-2.5"
        >
          <span className="min-w-0 flex-1 text-[13px] leading-snug text-[var(--ink)]">
            Echo stopped before it could transcribe this dictation. The audio was
            kept.
          </span>
          <button
            type="button"
            className="btn-ghost text-[13px]"
            disabled={busy}
            onClick={() => rescue.mutate(path)}
          >
            {rescue.isPending ? "Transcribing…" : "Transcribe it"}
          </button>
          <button
            type="button"
            className="btn-ghost text-[13px]"
            disabled={busy || discard.isPending}
            onClick={() => discard.mutate(path)}
          >
            Delete
          </button>
        </div>
      ))}

      <label className="flex items-start gap-2.5">
        <input
          type="checkbox"
          checked={labelling}
          disabled={!canLabel || busy}
          onChange={(e) => setSpeakers(e.target.checked)}
          className="mt-0.5 h-3.5 w-3.5 accent-white"
        />
        <span className="text-[14px] leading-snug">
          Label speakers
          <span className="block text-[12.5px] text-[var(--ink-muted)]">
            {speakersHint}
          </span>
        </span>
      </label>

      <button
        type="button"
        className="btn-ghost text-[13px]"
        disabled={busy}
        onClick={() => run.mutate()}
      >
        {run.isPending ? "Transcribing…" : "Choose an audio file…"}
      </button>

      {busy && name && (
        <p className="text-[12.5px] leading-relaxed text-[var(--ink-faint)]">
          Transcribing {name}. Long recordings take a while —{" "}
          {run.isPending && labelling && active
            ? `the file is being uploaded to ${active.label}.`
            : "this runs entirely on your machine."}
        </p>
      )}

      {(run.error ?? rescue.error ?? discard.error) != null && (
        <p className="text-[13px] leading-snug text-[var(--ink)]">
          {String(run.error ?? rescue.error ?? discard.error)}
        </p>
      )}

      {text !== null && (
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <span className="truncate text-[13px] font-medium text-[var(--ink-muted)]">
              {name}
            </span>
            <button type="button" className="btn-ghost text-[13px]" onClick={copy}>
              {copied ? "Copied" : "Copy"}
            </button>
          </div>
          <textarea
            readOnly
            value={text}
            rows={8}
            className="field w-full resize-y font-mono text-[13px] leading-relaxed"
          />
        </div>
      )}
    </div>
  );
}
