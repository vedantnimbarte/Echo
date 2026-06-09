import { useEffect } from "react";
import { echoEvents } from "../ipc/events";
import { commands } from "../ipc/commands";
import { useRecordingStore } from "../store/recordingStore";

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
    void commands.getSetting("recording_mode").then((m) => {
      if (m === "auto" || m === "manual") setMode(m);
    });
  }, [setMode]);

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onRecordingStarted(() => {
        setRecording(true);
        setTranscribing(false);
      }),
      echoEvents.onRecordingStopped(() => {
        setRecording(false);
        setSpeaking(false);
        // Manual stop kicks off transcription of the final buffer; in auto mode
        // per-utterance transcribing is driven by the speech-ended edge.
        if (useRecordingStore.getState().mode === "manual") setTranscribing(true);
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
        if (isRecording) void commands.stopRecording();
        else void commands.startRecording();
      }),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, [controlHotkey]);
}
