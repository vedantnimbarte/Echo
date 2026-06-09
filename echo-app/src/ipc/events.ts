import { listen, emit } from "@tauri-apps/api/event";
import type { RecordingMode } from "../store/recordingStore";

export interface TranscriptPartialPayload {
  type: "TranscriptPartial";
  payload: { text: string };
}

export interface TranscriptFinalPayload {
  type: "TranscriptFinal";
  payload: { text: string; language: string | null };
}

export const echoEvents = {
  onRecordingStarted: (cb: () => void) =>
    listen("echo://recording-started", cb),

  onRecordingStopped: (cb: () => void) =>
    listen("echo://recording-stopped", cb),

  onTranscriptPartial: (cb: (text: string) => void) =>
    listen<{ text: string }>("echo://transcript-partial", (e) =>
      cb(e.payload.text)
    ),

  onTranscriptFinal: (
    cb: (text: string, language: string | null) => void
  ) =>
    listen<{ text: string; language: string | null }>(
      "echo://transcript-final",
      (e) => cb(e.payload.text, e.payload.language)
    ),

  onError: (cb: (message: string) => void) =>
    listen<{ message: string }>("echo://error", (e) => cb(e.payload.message)),

  onModelDownloadProgress: (
    cb: (name: string, progress: number) => void
  ) =>
    listen<{ name: string; progress: number }>(
      "echo://model-download-progress",
      (e) => cb(e.payload.name, e.payload.progress)
    ),

  onModelDownloadComplete: (cb: (name: string) => void) =>
    listen<{ name: string }>("echo://model-download-complete", (e) =>
      cb(e.payload.name)
    ),

  onHotkeyToggle: (cb: () => void) => listen("echo://hotkey-toggle", cb),

  // Per-chunk RMS level (0..~1) of the audio currently being captured. Emitted
  // as a bare number so the pill can drive a live waveform.
  onAudioLevel: (cb: (level: number) => void) =>
    listen<number>("echo://audio-level", (e) => cb(e.payload)),

  // VAD edges — speech just started / stopped within the active session.
  onSpeechStarted: (cb: () => void) => listen("echo://speech-started", cb),
  onSpeechEnded: (cb: () => void) => listen("echo://speech-ended", cb),

  // Cross-window sync: the settings window broadcasts mode changes so the pill
  // updates live (separate webviews don't share a store).
  onModeChanged: (cb: (mode: RecordingMode) => void) =>
    listen<RecordingMode>("echo://mode-changed", (e) => cb(e.payload)),
  emitModeChanged: (mode: RecordingMode) => emit("echo://mode-changed", mode),
};
