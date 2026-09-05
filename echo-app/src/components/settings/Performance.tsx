import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { Group, Field, Check } from "../common/Page";

/**
 * Compute settings for the offline engine.
 *
 * Echo already prefers the GPU on its own, so this page is not where the user
 * turns acceleration on — it is where they find out *why* it is or is not
 * happening. That is the question a settings screen can actually answer:
 * "your machine has a CUDA GPU, the build for it is not downloaded yet, here
 * is the button." A silent automatic choice is only reassuring when you can
 * see what it chose.
 */
export function Performance() {
  const qc = useQueryClient();
  const [progress, setProgress] = useState<number | null>(null);

  const { data: gpu } = useQuery({
    queryKey: ["gpu-status"],
    queryFn: commands.gpuStatus,
  });

  const { data: warm } = useQuery({
    queryKey: ["setting", "warm_mic"],
    queryFn: () => commands.getSetting("warm_mic"),
  });

  useEffect(() => {
    const un = echoEvents.onWhisperBinaryProgress((p) =>
      setProgress(p >= 1 ? null : p),
    );
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const download = useMutation({
    mutationFn: () => commands.downloadGpuPack(),
    onSuccess: () => {
      setProgress(null);
      qc.invalidateQueries({ queryKey: ["gpu-status"] });
    },
    onError: () => setProgress(null),
  });

  const setEnabled = useMutation({
    mutationFn: (v: boolean) => commands.setGpuEnabled(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["gpu-status"] }),
  });

  const setThreads = useMutation({
    mutationFn: (v: string) => commands.setWhisperThreads(v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["gpu-status"] }),
  });

  const setWarm = useMutation({
    mutationFn: (v: boolean) =>
      commands.setSetting("warm_mic", v ? "true" : "false"),
    onSuccess: () =>
      qc.invalidateQueries({ queryKey: ["setting", "warm_mic"] }),
  });

  const canAccelerate = Boolean(gpu?.available_pack);
  const busy = download.isPending || progress !== null;

  return (
    <>
      <Group
        title="Processing"
        hint={
          <>
            Echo uses the GPU whenever it can and falls back to the CPU on its
            own, so this is here to show what it picked rather than to be
            configured. Turning it off and on again also clears a failure, which
            is the way to retry after fixing a driver.
          </>
        }
      >
        <div className="rounded-lg border border-[var(--hairline)] bg-[var(--surface-1)] px-3.5 py-3">
          <div className="flex items-baseline justify-between gap-4">
            <span className="text-[12px] font-medium text-[var(--ink)]">
              {gpu?.detected ?? "Checking…"}
            </span>
            <span className="text-[10.5px] text-[var(--ink-faint)]">
              {statusLabel(gpu)}
            </span>
          </div>

          {gpu?.failed && (
            <p className="mt-2 text-[10.5px] leading-relaxed text-[var(--ink-muted)]">
              The accelerated build failed to run, so Echo switched to the CPU
              for this session. Toggling GPU acceleration off and on tries it
              again.
            </p>
          )}

          {canAccelerate && !gpu?.pack_installed && (
            <div className="mt-2.5 space-y-2">
              <p className="text-[10.5px] leading-relaxed text-[var(--ink-muted)]">
                A build for your GPU is available and will make local
                transcription substantially faster.
                {gpu?.available_pack_mb
                  ? ` It is a ${gpu.available_pack_mb} MB download.`
                  : ""}
              </p>
              <button
                type="button"
                className="btn-primary text-[11px]"
                disabled={busy}
                onClick={() => download.mutate()}
              >
                {busy
                  ? progress !== null
                    ? `Downloading… ${Math.round(progress * 100)}%`
                    : "Downloading…"
                  : "Download GPU build"}
              </button>
              {download.error != null && (
                <p className="text-[10.5px] leading-relaxed text-[var(--ink)]">
                  {String(download.error)}
                </p>
              )}
            </div>
          )}
        </div>

        <Check
          checked={gpu?.enabled !== false}
          onChange={(v) => setEnabled.mutate(v)}
        >
          Use the GPU when one is available
        </Check>

        <Field label="Decode threads">
          <select
            className="field w-full"
            value={String(gpu?.threads ?? "auto")}
            onChange={(e) => setThreads.mutate(e.target.value)}
          >
            <option value="auto">Automatic</option>
            {[1, 2, 4, 6, 8, 12, 16].map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
        </Field>
      </Group>

      <Group
        title="Responsiveness"
        hint={
          <>
            Keeping the microphone open for a few seconds after you stop lets
            the next sentence start instantly, and captures the moment just
            before you press the key — so a word begun early is not cut off.
            While it is open, your system will show the microphone as in use.
          </>
        }
      >
        <Check checked={warm !== "false"} onChange={(v) => setWarm.mutate(v)}>
          Keep the microphone ready between dictations
        </Check>
      </Group>
    </>
  );
}

function statusLabel(gpu?: {
  active: boolean;
  failed: boolean;
  enabled: boolean;
  available_pack: string | null;
  pack_installed: boolean;
}): string {
  if (!gpu) return "";
  if (gpu.active) return "Accelerated";
  if (gpu.failed) return "Fell back to CPU";
  if (!gpu.enabled) return "GPU turned off";
  if (gpu.available_pack && !gpu.pack_installed) return "Build not installed";
  return "Running on CPU";
}
