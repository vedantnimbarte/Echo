import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";

import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";

/**
 * What is turning speech into words, right now.
 *
 * Two facts: where the words are made, and which model makes them. The dot
 * carries the one that matters — filled means the audio is staying on this
 * machine, hollow means it is being sent to someone else's.
 *
 * The dot answers for *this moment*, not for the setting. A cloud provider
 * with no key never receives anything, and one that just failed an utterance
 * has already been replaced by the offline engine — both are a filled dot,
 * whatever the Engine page says is selected. A tag that reported the
 * preference instead would be wrong at exactly the times it is worth reading.
 *
 * Colour carries none of this. The single chromatic value in Echo belongs to a
 * live microphone; a green/amber pair here would both borrow it and imply one
 * choice is a warning, which is not what the user chose.
 */
export interface EngineStatus {
  /** Whether the audio is staying on this machine — what the dot reports. */
  onDevice: boolean;
  /** Who is answering. */
  where: string;
  /** Which model, or what is missing before there can be one. */
  detail: string;
  /** The chosen engine cannot answer until something is fixed. */
  needsAttention: boolean;
  /**
   * The whole thing as a sentence, with no call to action — the pill's
   * microphone button borrows this, and clicking that starts dictation rather
   * than opening anything.
   */
  summary: string;
  /** `summary`, plus what clicking the tag will do. */
  title: string;
}

/**
 * Resolves the engine the way the backend would, not the way settings store it.
 *
 * Three sources, because the setting alone cannot tell you whether it works:
 * the provider catalog knows whether a cloud key is in the keychain, and
 * `whisperReady` knows whether the offline engine has both its binary and its
 * model. The fourth is live — `core/asr/fallback.rs` reports a diverted
 * utterance, and that outranks all of them.
 */
export function useEngineStatus(): EngineStatus | null {
  const qc = useQueryClient();

  const { data: provider } = useQuery({
    queryKey: ["setting", "asr_provider"],
    queryFn: () => commands.getSetting("asr_provider"),
  });
  const { data: whisperModel } = useQuery({
    queryKey: ["setting", "whisper_model"],
    queryFn: () => commands.getSetting("whisper_model"),
  });
  const { data: cloud } = useQuery({
    queryKey: ["cloud-providers"],
    queryFn: commands.listCloudProviders,
  });
  const { data: localReady } = useQuery({
    queryKey: ["whisper-ready"],
    queryFn: commands.whisperReady,
  });

  // Latched between utterances rather than shown per event: one diverted
  // utterance means the provider is not answering, and it will still not be
  // answering for the next one. Cleared by a final transcript that did *not*
  // divert — the only evidence we get that the chosen engine works again.
  const divertedThisUtterance = useRef(false);
  const [diverted, setDiverted] = useState(false);

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onAsrFellBack(() => {
        divertedThisUtterance.current = true;
        setDiverted(true);
      }),
      echoEvents.onTranscriptFinal(() => {
        setDiverted(divertedThisUtterance.current);
        divertedThisUtterance.current = false;
      }),
      // The settings window owns the choice; this is also rendered in the
      // pill, which is a separate webview with its own query cache.
      echoEvents.onEngineChanged(() => {
        setDiverted(false);
        divertedThisUtterance.current = false;
        qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
        qc.invalidateQueries({ queryKey: ["setting", "whisper_model"] });
        qc.invalidateQueries({ queryKey: ["cloud-providers"] });
        qc.invalidateQueries({ queryKey: ["whisper-ready"] });
      }),
    ]);
    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [qc]);

  // Nothing until the answer is known. A "Local" default that flipped to a
  // cloud name a frame later would be a lie told on exactly the point this
  // exists to be trusted about.
  if (provider === undefined || cloud === undefined) return null;

  const remote = cloud.find((p) => p.id === provider) ?? null;
  // Nothing chosen yet is base.en — the same default the model shelf lights up.
  const localModel = whisperModel || "base.en";

  if (!remote) {
    return localReady === false
      ? {
          onDevice: true,
          where: "Local",
          detail: "Not ready",
          needsAttention: true,
          summary:
            "The offline engine is not set up yet, so nothing can be transcribed.",
          title:
            "The offline engine is not set up yet, so nothing can be transcribed. Click to finish it.",
        }
      : {
          onDevice: true,
          where: "Local",
          detail: localModel,
          needsAttention: false,
          summary: "Dictation runs on this machine. Your audio stays here.",
          title:
            "Dictation runs on this machine. Your audio stays here. Click to change it.",
        };
  }

  if (diverted) {
    return {
      onDevice: true,
      where: `${remote.label} → Local`,
      detail: localModel,
      needsAttention: true,
      summary: `${remote.label} did not answer, so Echo transcribed on this machine instead. Your audio stayed here.`,
      title: `${remote.label} did not answer, so Echo transcribed on this machine instead. Your audio stayed here. Click to check it.`,
    };
  }

  if (!remote.available) {
    return {
      onDevice: true,
      where: remote.label,
      detail: "Unavailable",
      needsAttention: true,
      summary: `${remote.label} is not built into this version of Echo, so the offline engine is doing the work.`,
      title: `${remote.label} is not built into this version of Echo, so the offline engine is doing the work. Click to pick another.`,
    };
  }

  if (!remote.key_set) {
    return {
      onDevice: true,
      where: remote.label,
      detail: "Needs a key",
      needsAttention: true,
      summary: `${remote.label} needs an API key before it can transcribe. Until then the offline engine does the work and your audio stays here.`,
      title: `${remote.label} needs an API key before it can transcribe. Until then the offline engine does the work and your audio stays here. Click to add one.`,
    };
  }

  return {
    onDevice: false,
    where: remote.label,
    detail: remote.model,
    needsAttention: false,
    summary: `Dictation runs on ${remote.label}. Your audio leaves this machine.`,
    title: `Dictation runs on ${remote.label}. Your audio leaves this machine. Click to change it.`,
  };
}

/**
 * The tag itself.
 *
 * `bare` drops the capsule for contexts that are already one — the floating
 * pill — where a bordered chip inside a bordered chip reads as two objects
 * rather than as one instrument.
 */
export function EngineTag({
  onOpen,
  bare,
  revealed = true,
  className,
}: {
  onOpen: () => void;
  bare?: boolean;
  /**
   * False in a chrome that only offers the tag on approach — the pill. An
   * engine that needs fixing ignores it: that is news, and news should not
   * wait for someone to happen to point at it.
   */
  revealed?: boolean;
  className?: string;
}) {
  const status = useEngineStatus();
  if (!status || (!revealed && !status.needsAttention)) return null;

  return (
    <button
      onClick={onOpen}
      title={status.title}
      aria-label={status.title}
      className={clsx(
        "group flex max-w-[min(46vw,280px)] shrink-0 items-center gap-2 rounded-full text-[11.5px] tracking-tight transition-colors",
        bare
          ? "px-1.5 py-0.5 text-[var(--ink-muted)] hover:bg-[var(--surface-2)] hover:text-[var(--ink)]"
          : "border border-[var(--hairline)] py-[3px] pl-2 pr-2.5 text-[var(--ink-muted)] hover:border-[var(--hairline-strong)] hover:bg-[var(--surface-1)] hover:text-[var(--ink)]",
        // Something to fix reads at full strength; a working engine is
        // reference rather than news, and sits back with the rest of the frame.
        status.needsAttention && "text-[var(--ink)]",
        className
      )}
    >
      <svg viewBox="0 0 8 8" className="h-[7px] w-[7px] shrink-0" aria-hidden="true">
        <circle
          cx="4"
          cy="4"
          r="2.5"
          fill={status.onDevice ? "currentColor" : "none"}
          stroke="currentColor"
          strokeWidth="1"
        />
      </svg>

      <span className="truncate">{status.where}</span>

      {status.detail && (
        <>
          <span
            aria-hidden
            className="h-[11px] w-px shrink-0 bg-[var(--hairline)] transition-colors group-hover:bg-[var(--hairline-strong)]"
          />
          <span
            className={clsx(
              "truncate transition-colors",
              status.needsAttention
                ? "text-[var(--ink-muted)] group-hover:text-[var(--ink)]"
                : "text-[var(--ink-faint)] group-hover:text-[var(--ink-muted)]"
            )}
          >
            {status.detail}
          </span>
        </>
      )}
    </button>
  );
}
