import { useEffect } from "react";
import { echoEvents } from "../ipc/events";
import { commands } from "../ipc/commands";
import { useRecordingStore } from "../store/recordingStore";

export function useEchoEvents() {
  const {
    setRecording,
    setPartialTranscript,
    appendFinalTranscript,
    setError,
  } = useRecordingStore();

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onRecordingStarted(() => setRecording(true)),
      echoEvents.onRecordingStopped(() => setRecording(false)),
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
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);
}
