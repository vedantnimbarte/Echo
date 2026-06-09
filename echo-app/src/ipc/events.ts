import { listen } from "@tauri-apps/api/event";

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
};
