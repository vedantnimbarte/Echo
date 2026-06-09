import { create } from "zustand";

/**
 * "manual"  — push-to-talk: the user starts/stops via hotkey or the pill.
 * "auto"    — voice-activated: armed once, then each utterance is captured and
 *             transcribed automatically as the user speaks and pauses.
 */
export type RecordingMode = "manual" | "auto";

interface RecordingState {
  /** Capture session is running (armed, in auto mode). */
  isRecording: boolean;
  /** Speech is currently detected (VAD rising/falling edges). */
  speaking: boolean;
  /** Between an utterance ending and its transcript arriving. */
  transcribing: boolean;
  mode: RecordingMode;
  partialTranscript: string;
  finalTranscript: string;
  language: string | null;
  error: string | null;

  setRecording: (v: boolean) => void;
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
  speaking: false,
  transcribing: false,
  mode: "manual",
  partialTranscript: "",
  finalTranscript: "",
  language: null,
  error: null,

  setRecording: (v) => set({ isRecording: v }),
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
