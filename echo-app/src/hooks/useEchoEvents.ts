import { useEffect } from "react";
import { echoEvents } from "../ipc/events";
import { commands } from "../ipc/commands";
import { normalizeMode, useRecordingStore } from "../store/recordingStore";
import { errorMessage } from "../lib/errors";

interface Options {
  /**
   * Whether this window owns the global hotkey toggle. Both webview windows
   * receive the broadcast, so exactly one (the pill) should act on it to avoid
   * double-triggering recording transitions.
   */
  controlHotkey?: boolean;
}

export function useEchoEvents({ controlHotkey = false }: Options = {}) {
  const {
    setRecording,
    setSpeaking,
    setTranscribing,
    setMode,
    setPartialTranscript,
    appendFinalTranscript,
    setError,
  } = useRecordingStore();

  // Load the persisted recording mode once.
  useEffect(() => {
    void commands.getSetting("recording_mode").then((m) => setMode(normalizeMode(m)));
  }, [setMode]);

  useEffect(() => {
    const report = (e: unknown) => setError(errorMessage(e));
    const unlisten = Promise.all([
      echoEvents.onRecordingStarted(() => {
        setRecording(true);
        setTranscribing(false);
      }),
      echoEvents.onRecordingStopped(() => {
        setRecording(false);
        setSpeaking(false);
        // Stopping by hand kicks off transcription of the final buffer; in
        // auto mode per-utterance transcribing follows the speech-ended edge.
        if (useRecordingStore.getState().mode !== "auto") setTranscribing(true);
      }),
      echoEvents.onSpeechStarted(() => {
        setSpeaking(true);
        setTranscribing(false);
      }),
      echoEvents.onSpeechEnded(() => {
        setSpeaking(false);
        setTranscribing(true);
      }),
      echoEvents.onTranscriptPartial((text) => setPartialTranscript(text)),
      echoEvents.onTranscriptFinal((text, language) => {
        appendFinalTranscript(text, language);
        // Record only non-sensitive metadata — never the transcript text.
        const wordCount = text.trim() ? text.trim().split(/\s+/).length : 0;
        void commands.recordTelemetryEvent("transcription_complete", {
          word_count: wordCount,
          language,
        });
      }),
      echoEvents.onError((msg) => setError(msg)),
      echoEvents.onModeChanged((mode) => setMode(mode)),
      echoEvents.onHotkeyToggle(() => {
        if (!controlHotkey) return;
        const { isRecording } = useRecordingStore.getState();
        // Failures here were dropped on the floor: a microphone that will not
        // open left the hotkey doing nothing, silently, for the rest of the
        // session. The pill's own button has always reported this.
        if (isRecording) void commands.stopRecording().catch(report);
        else void commands.startRecording().catch(report);
      }),
      // Hold-to-talk. Guarded on the current state either way: the key can be
      // released after a stop has already happened some other way.
      echoEvents.onHotkeyPress(() => {
        if (!controlHotkey) return;
        if (!useRecordingStore.getState().isRecording)
          void commands.startRecording().catch(report);
      }),
      echoEvents.onHotkeyRelease(() => {
        if (!controlHotkey) return;
        if (useRecordingStore.getState().isRecording)
          void commands.stopRecording().catch(report);
      }),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [controlHotkey]);
}
