import { Mic, MicOff } from "lucide-react";
import clsx from "clsx";
import { useQuery } from "@tanstack/react-query";
import { useRecordingStore } from "../../store/recordingStore";
import { commands } from "../../ipc/commands";

export function RecordingPanel() {
  const {
    isRecording,
    mode,
    selectedDevice,
    partialTranscript,
    finalTranscript,
    error,
    setMode,
    setSelectedDevice,
  } = useRecordingStore();

  const { data: devices = [] } = useQuery({
    queryKey: ["audio-devices"],
    queryFn: commands.getAudioDevices,
  });

  async function toggleRecording() {
    if (isRecording) {
      await commands.stopRecording();
    } else {
      await commands.startRecording(selectedDevice ?? undefined);
    }
  }

  return (
    <div className="flex flex-col items-center gap-6 p-8">
      {/* Mode toggle */}
      <div className="flex rounded-lg border border-zinc-700 overflow-hidden text-sm">
        {(["toggle", "push-to-talk"] as const).map((m) => (
          <button
            key={m}
            onClick={() => setMode(m)}
            className={clsx(
              "px-4 py-1.5 transition-colors",
              mode === m
                ? "bg-violet-600 text-white"
                : "text-zinc-400 hover:text-zinc-200"
            )}
          >
            {m === "toggle" ? "Toggle" : "Push-to-Talk"}
          </button>
        ))}
      </div>

      {/* Microphone selector */}
      <div className="flex flex-col items-center gap-1">
        <label className="text-xs text-zinc-500">Microphone</label>
        <select
          value={selectedDevice ?? ""}
          onChange={(e) => setSelectedDevice(e.target.value || null)}
          disabled={isRecording}
          className="min-w-[220px] rounded-lg border border-zinc-700 bg-zinc-800 px-3 py-1.5 text-sm text-zinc-200 disabled:opacity-50"
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.name} value={d.name}>
              {d.name}
              {d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>
      </div>

      {/* Record button */}
      <button
        onClick={toggleRecording}
        className={clsx(
          "w-24 h-24 rounded-full flex items-center justify-center transition-all shadow-lg",
          isRecording
            ? "bg-red-500 hover:bg-red-600 scale-110 ring-4 ring-red-400/40"
            : "bg-violet-600 hover:bg-violet-700"
        )}
        aria-label={isRecording ? "Stop recording" : "Start recording"}
      >
        {isRecording ? (
          <MicOff className="w-10 h-10 text-white" />
        ) : (
          <Mic className="w-10 h-10 text-white" />
        )}
      </button>

      <p className="text-sm text-zinc-500">
        {isRecording ? "Recording…" : "Click to record"}
      </p>

      {/* Transcript display */}
      <div className="w-full max-w-lg min-h-[80px] rounded-xl bg-zinc-800/50 border border-zinc-700 p-4 font-mono text-sm">
        {finalTranscript && (
          <span className="text-zinc-100">{finalTranscript} </span>
        )}
        {partialTranscript && (
          <span className="text-zinc-400 italic">{partialTranscript}</span>
        )}
        {!finalTranscript && !partialTranscript && (
          <span className="text-zinc-600">Transcript will appear here…</span>
        )}
      </div>

      {error && (
        <p className="text-red-400 text-sm">{error}</p>
      )}
    </div>
  );
}
