import { useEffect } from "react";
import { echoEvents } from "../ipc/events";
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
      echoEvents.onTranscriptFinal((text, language) =>
        appendFinalTranscript(text, language)
      ),
      echoEvents.onError((msg) => setError(msg)),
    ]);

    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);
}
