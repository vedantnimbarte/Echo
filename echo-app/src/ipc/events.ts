import { listen, emit } from "@tauri-apps/api/event";
import type { RecordingMode } from "../store/recordingStore";
import type { PillSize } from "../components/pill/Pill";

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

  // whisper-cli binary download progress (bare 0..1 fraction).
  onWhisperBinaryProgress: (cb: (progress: number) => void) =>
    listen<number>("echo://whisper-binary-progress", (e) => cb(e.payload)),

  onHotkeyToggle: (cb: () => void) => listen("echo://hotkey-toggle", cb),

  // Hold-to-talk: these bracket one utterance, rather than toggling.
  onHotkeyPress: (cb: () => void) => listen("echo://hotkey-press", cb),
  onHotkeyRelease: (cb: () => void) => listen("echo://hotkey-release", cb),

  // The wake phrase was spoken; dictation is about to start.
  onWakeDetected: (cb: (phrase: string, score: number) => void) =>
    listen<{ phrase: string; score: number }>("echo://wake-detected", (e) =>
      cb(e.payload.phrase, e.payload.score)
    ),

  // Wake-model download progress (bare 0..1 fraction).
  onWakeModelProgress: (cb: (progress: number) => void) =>
    listen<number>("echo://wake-model-progress", (e) => cb(e.payload)),

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

  // The chosen engine failed an utterance and the offline engine answered
  // instead. Fired before the retry, so the screen stops naming a provider
  // that is no longer doing the work. Carries the provider that was dropped.
  onAsrFellBack: (cb: (provider: string) => void) =>
    listen<{ provider: string }>("echo://asr-fell-back", (e) =>
      cb(e.payload.provider)
    ),

  // Which engine is in use is chosen in the settings window and reported by
  // both windows, so the change is broadcast for the same reason pill size is.
  onEngineChanged: (cb: () => void) => listen("echo://engine-changed", cb),
  emitEngineChanged: () => emit("echo://engine-changed"),

  // Pill size lives in the settings window but is rendered by the pill, and the
  // two are separate webviews with separate stores — so the change is
  // broadcast rather than read back on a timer.
  onPillSizeChanged: (cb: (size: PillSize) => void) =>
    listen<PillSize>("echo://pill-size-changed", (e) => cb(e.payload)),
  emitPillSizeChanged: (size: PillSize) => emit("echo://pill-size-changed", size),
};
