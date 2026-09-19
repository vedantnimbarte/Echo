import { create } from "zustand";

/**
 * "hold"    — record only while the hotkey is held down, the way push-to-talk
 *             conventionally works.
 * "toggle"  — tap the hotkey to start, tap again to stop.
 * "auto"    — voice-activated: armed once, then each utterance is captured and
 *             transcribed automatically as the user speaks and pauses.
 */
export type RecordingMode = "hold" | "toggle" | "auto";

/**
 * Read a stored mode. `"manual"` was what tap-to-toggle was called before hold
 * existed, so it maps forward rather than resetting anyone's setting.
 */
export function normalizeMode(raw: string | null | undefined): RecordingMode {
  if (raw === "auto") return "auto";
  if (raw === "hold") return "hold";
  return "toggle";
}

/** Mirrors DictationState in src-tauri/src/core/dictation/machine.rs. */
export type DictationState =
  | "IDLE"
  | "ARMING"
  | "RECORDING"
  | "CANCEL_ARMED"
  | "FINALIZING";

interface RecordingState {
  /** Capture session is running (armed, in auto mode). */
  isRecording: boolean;
  /** Speech is currently detected (VAD rising/falling edges). */
  speaking: boolean;
  /** Between an utterance ending and its transcript arriving. */
  transcribing: boolean;
  /**
   * The dictation state machine's own view, pushed from Rust.
   *
   * Separate from `isRecording` rather than derived from it, because the two
   * answer different questions: `isRecording` is "is the pill showing a live
   * dictation", which stays true through a cancel countdown, while this says
   * WHICH live state it is in. Collapsing them would make the countdown
   * indistinguishable from ordinary recording, which is the one thing the
   * cancel UI exists to show.
   */
  dictation: DictationState;
  /** How long the countdown runs, as the backend will actually time it. */
  cancelCountdownMs: number;
  mode: RecordingMode;
  partialTranscript: string;
  finalTranscript: string;
  language: string | null;
  error: string | null;

  setRecording: (v: boolean) => void;
  setDictation: (state: DictationState, cancelCountdownMs: number) => void;
  setSpeaking: (v: boolean) => void;
  setTranscribing: (v: boolean) => void;
  setMode: (mode: RecordingMode) => void;
  setPartialTranscript: (text: string) => void;
  appendFinalTranscript: (text: string, language: string | null) => void;
  clearTranscript: () => void;
  setError: (msg: string | null) => void;
}

export const useRecordingStore = create<RecordingState>((set) => ({
  isRecording: false,
  dictation: "IDLE",
  cancelCountdownMs: 3000,
  speaking: false,
  transcribing: false,
  mode: "toggle",
  partialTranscript: "",
  finalTranscript: "",
  language: null,
  error: null,

  setRecording: (v) => set({ isRecording: v }),
  setDictation: (dictation, cancelCountdownMs) => set({ dictation, cancelCountdownMs }),
  setSpeaking: (speaking) => set({ speaking }),
  setTranscribing: (transcribing) => set({ transcribing }),
  setMode: (mode) => set({ mode }),
  setPartialTranscript: (text) => set({ partialTranscript: text }),
  appendFinalTranscript: (text, language) =>
    set((s) => ({
      finalTranscript: s.finalTranscript ? `${s.finalTranscript} ${text}` : text,
      partialTranscript: "",
      transcribing: false,
      language,
    })),
  clearTranscript: () =>
    set({ partialTranscript: "", finalTranscript: "", language: null }),
  setError: (error) => set({ error, transcribing: false }),
}));
