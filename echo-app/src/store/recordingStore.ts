import { create } from "zustand";

type RecordingMode = "push-to-talk" | "toggle";

interface RecordingState {
  isRecording: boolean;
  mode: RecordingMode;
  selectedDevice: string | null;
  partialTranscript: string;
  finalTranscript: string;
  language: string | null;
  error: string | null;

  setRecording: (v: boolean) => void;
  setMode: (mode: RecordingMode) => void;
  setSelectedDevice: (name: string | null) => void;
  setPartialTranscript: (text: string) => void;
  appendFinalTranscript: (text: string, language: string | null) => void;
  clearTranscript: () => void;
  setError: (msg: string | null) => void;
}

export const useRecordingStore = create<RecordingState>((set) => ({
  isRecording: false,
  mode: "toggle",
  selectedDevice: null,
  partialTranscript: "",
  finalTranscript: "",
  language: null,
  error: null,

  setRecording: (v) => set({ isRecording: v }),
  setMode: (mode) => set({ mode }),
  setSelectedDevice: (selectedDevice) => set({ selectedDevice }),
  setPartialTranscript: (text) => set({ partialTranscript: text }),
  appendFinalTranscript: (text, language) =>
    set((s) => ({
      finalTranscript: s.finalTranscript ? `${s.finalTranscript} ${text}` : text,
      partialTranscript: "",
      language,
    })),
  clearTranscript: () =>
    set({ partialTranscript: "", finalTranscript: "", language: null }),
  setError: (error) => set({ error }),
}));
