import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import {
  Mic,
  ShieldCheck,
  Download,
  Keyboard,
  Sparkles,
  Check,
  Loader2,
  ArrowRight,
  ArrowLeft,
} from "lucide-react";

import { commands } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { Waveform } from "../pill/Waveform";
import { CloudProviders } from "../settings/CloudProviders";
import { HotkeyCapture } from "../common/HotkeyCapture";

type StepId = "welcome" | "mic" | "engine" | "permissions" | "hotkey";
const STEPS: { id: StepId; label: string; Icon: React.ElementType }[] = [
  { id: "welcome", label: "Welcome", Icon: Sparkles },
  { id: "mic", label: "Microphone", Icon: Mic },
  { id: "engine", label: "Transcription", Icon: Download },
  { id: "permissions", label: "Permissions", Icon: ShieldCheck },
  { id: "hotkey", label: "Shortcut", Icon: Keyboard },
];

export function Onboarding({ onDone }: { onDone: () => void }) {
  const qc = useQueryClient();
  const [stepIdx, setStepIdx] = useState(0);
  const step = STEPS[stepIdx].id;

  async function finish() {
    await commands.setSetting("onboarding_complete", "true");
    qc.invalidateQueries({ queryKey: ["setting", "onboarding_complete"] });
    onDone();
  }

  const next = () => setStepIdx((i) => Math.min(i + 1, STEPS.length - 1));
  const back = () => setStepIdx((i) => Math.max(i - 1, 0));
  const isLast = stepIdx === STEPS.length - 1;

  return (
    <div className="relative flex h-screen flex-col overflow-hidden bg-[#0a0b11] text-[var(--ink)]">
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-64 opacity-60"
        style={{
          background:
            "radial-gradient(60% 100% at 30% 0%, rgba(52,231,228,0.14), transparent 70%), radial-gradient(60% 100% at 80% 0%, rgba(160,107,255,0.14), transparent 70%)",
        }}
      />

      {/* Step rail */}
      <div className="relative flex items-center justify-center gap-2 pt-6">
        {STEPS.map((s, i) => (
          <div key={s.id} className="flex items-center gap-2">
            <span
              className={clsx(
                "flex h-7 w-7 items-center justify-center rounded-full border text-[11px] transition",
                i < stepIdx && "border-[var(--aurora-1)]/50 bg-[var(--aurora-1)]/15 text-[var(--aurora-1)]",
                i === stepIdx && "border-[var(--aurora-2)] bg-[var(--aurora-2)]/20 text-[var(--ink)]",
                i > stepIdx && "border-white/10 text-[var(--ink-faint)]"
              )}
            >
              {i < stepIdx ? <Check className="h-3.5 w-3.5" /> : <s.Icon className="h-3.5 w-3.5" />}
            </span>
            {i < STEPS.length - 1 && (
              <span
                className={clsx(
                  "h-px w-7 transition",
                  i < stepIdx ? "bg-[var(--aurora-1)]/40" : "bg-white/10"
                )}
              />
            )}
          </div>
        ))}
      </div>

      {/* Step body */}
      <div className="relative flex min-h-0 flex-1 items-center justify-center px-6">
        <div className="w-full max-w-[440px]">
          {step === "welcome" && <WelcomeStep />}
          {step === "mic" && <MicStep />}
          {step === "engine" && <EngineStep />}
          {step === "permissions" && <PermissionsStep />}
          {step === "hotkey" && <HotkeyStep />}
        </div>
      </div>

      {/* Nav */}
      <div className="relative flex items-center justify-between border-t border-white/6 px-6 py-4">
        <button
          onClick={back}
          disabled={stepIdx === 0}
          className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[12px] text-[var(--ink-muted)] transition hover:text-[var(--ink)] disabled:opacity-0"
        >
          <ArrowLeft className="h-3.5 w-3.5" /> Back
        </button>
        <button
          onClick={finish}
          className="text-[11px] text-[var(--ink-faint)] transition hover:text-[var(--ink-muted)]"
        >
          Skip setup
        </button>
        <button
          onClick={isLast ? finish : next}
          className="flex items-center gap-1.5 rounded-lg bg-[var(--aurora-2)] px-4 py-1.5 text-[12px] font-medium text-white transition hover:brightness-110"
        >
          {isLast ? "Finish" : "Continue"}
          {!isLast && <ArrowRight className="h-3.5 w-3.5" />}
        </button>
      </div>
    </div>
  );
}

/* ---- steps ---------------------------------------------------------------- */

function StepHeading({ title, sub }: { title: string; sub: string }) {
  return (
    <div className="mb-5 text-center">
      <h2 className="text-[20px] font-semibold tracking-tight">{title}</h2>
      <p className="mx-auto mt-1.5 max-w-[360px] text-[13px] leading-snug text-[var(--ink-muted)]">
        {sub}
      </p>
    </div>
  );
}

function WelcomeStep() {
  return (
    <div className="text-center">
      <div
        className="mx-auto mb-5 flex h-14 w-14 items-center justify-center rounded-2xl"
        style={{
          background: "linear-gradient(135deg, var(--aurora-1), var(--aurora-3))",
          boxShadow: "0 12px 40px -8px rgba(91,141,239,0.5)",
        }}
      >
        <Sparkles className="h-7 w-7 text-white" />
      </div>
      <StepHeading
        title="Welcome to Echo"
        sub="Your voice, typed into any app. Echo transcribes on your device by default — nothing leaves your machine unless you add a cloud key. Let's get you set up in a few quick steps."
      />
      <div className="mx-auto max-w-[360px] space-y-2 text-left">
        {[
          "Private by default — local Whisper, offline",
          "Works in any app via your global shortcut",
          "Optional cloud engines for speed & accuracy",
        ].map((t) => (
          <div key={t} className="flex items-center gap-2.5 text-[12.5px] text-[var(--ink-muted)]">
            <Check className="h-4 w-4 shrink-0 text-[var(--aurora-1)]" />
            {t}
          </div>
        ))}
      </div>
    </div>
  );
}

function MicStep() {
  const [testing, setTesting] = useState(false);
  const { data: devices = [] } = useQuery({
    queryKey: ["audio-devices"],
    queryFn: commands.getAudioDevices,
  });
  const { data: savedDevice } = useQuery({
    queryKey: ["setting", "audio_device"],
    queryFn: () => commands.getSetting("audio_device"),
  });
  const qc = useQueryClient();

  // Start/stop a live capture so the meter reflects the real mic. At this point
  // the local engine usually isn't provisioned yet, so nothing is transcribed.
  useEffect(() => {
    if (!testing) return;
    void commands.startRecording().catch(() => setTesting(false));
    return () => {
      void commands.stopRecording();
    };
  }, [testing]);

  return (
    <div>
      <StepHeading title="Pick your microphone" sub="Choose an input and test that Echo hears you." />
      <div className="space-y-3">
        <select
          className="w-full rounded-lg border border-white/10 bg-white/5 px-2.5 py-2 text-[13px] text-[var(--ink)] outline-none focus:border-[var(--aurora-2)]/60"
          value={savedDevice ?? ""}
          onChange={(e) =>
            void commands.setSetting("audio_device", e.target.value).then(() =>
              qc.invalidateQueries({ queryKey: ["setting", "audio_device"] })
            )
          }
        >
          <option value="">System default</option>
          {devices.map((d) => (
            <option key={d.name} value={d.name}>
              {d.name}
              {d.is_default ? " (default)" : ""}
            </option>
          ))}
        </select>

        <div className="flex items-center justify-between rounded-xl border border-white/8 bg-white/[0.025] px-4 py-3">
          <div className="flex h-6 items-center">
            {testing ? (
              <Waveform mode="listening" />
            ) : (
              <span className="text-[12px] text-[var(--ink-faint)]">Meter idle</span>
            )}
          </div>
          <button
            onClick={() => setTesting((t) => !t)}
            className={clsx(
              "rounded-lg px-3 py-1.5 text-[12px] font-medium transition",
              testing
                ? "bg-[var(--rec)] text-white"
                : "border border-white/12 text-[var(--ink)] hover:bg-white/8"
            )}
          >
            {testing ? "Stop test" : "Test microphone"}
          </button>
        </div>
      </div>
    </div>
  );
}

function EngineStep() {
  const qc = useQueryClient();
  const [binProgress, setBinProgress] = useState<number | null>(null);
  const [modelProgress, setModelProgress] = useState<number | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);

  const { data: ready, refetch } = useQuery({
    queryKey: ["whisper-ready"],
    queryFn: commands.whisperReady,
  });

  useEffect(() => {
    const unlisten = Promise.all([
      echoEvents.onWhisperBinaryProgress((p) => setBinProgress(p)),
      echoEvents.onModelDownloadProgress((name, p) => {
        if (name === "base.en") setModelProgress(p);
      }),
    ]);
    return () => {
      unlisten.then((fns) => fns.forEach((fn) => fn()));
    };
  }, []);

  async function provision() {
    setBusy(true);
    setNote(null);
    try {
      // 1) The whisper-cli binary (Windows auto-downloads; other OSes need it on PATH).
      try {
        setBinProgress(0);
        await commands.downloadWhisperBinary();
      } catch (e) {
        setBinProgress(null);
        setNote(
          "Couldn't auto-install the Whisper binary on this platform. Install a `whisper-cli` on your PATH, or add a cloud key below."
        );
      }
      // 2) The default model.
      const models = await commands.listModels();
      const base = models.find((m) => m.name === "base.en");
      if (base && !base.downloaded) {
        setModelProgress(0);
        await commands.downloadModel("base.en");
      }
      // 3) Activate local if everything is in place.
      if (await commands.whisperReady()) {
        await commands.setWhisperModel("base.en");
        await commands.setAsrProvider("local");
        qc.invalidateQueries({ queryKey: ["setting", "asr_provider"] });
      }
    } finally {
      setBinProgress(null);
      setModelProgress(null);
      setBusy(false);
      void refetch();
    }
  }

  return (
    <div>
      <StepHeading
        title="Set up transcription"
        sub="Echo runs Whisper locally by default. We'll download a small English model (~142 MB) once."
      />

      {ready ? (
        <div className="flex items-center gap-2.5 rounded-xl border border-emerald-500/30 bg-emerald-500/10 px-4 py-3 text-[13px] text-emerald-300">
          <Check className="h-4 w-4" /> Local transcription is ready.
        </div>
      ) : (
        <div className="space-y-3">
          <button
            onClick={provision}
            disabled={busy}
            className="flex w-full items-center justify-center gap-2 rounded-xl bg-[var(--aurora-2)] px-4 py-2.5 text-[13px] font-medium text-white transition hover:brightness-110 disabled:opacity-60"
          >
            {busy ? <Loader2 className="h-4 w-4 animate-spin" /> : <Download className="h-4 w-4" />}
            {busy ? "Setting up…" : "Set up local Whisper"}
          </button>

          {binProgress !== null && (
            <ProgressRow label="Whisper engine" value={binProgress} />
          )}
          {modelProgress !== null && (
            <ProgressRow label="base.en model" value={modelProgress} />
          )}
          {note && <p className="text-[11px] leading-snug text-amber-300/90">{note}</p>}
        </div>
      )}

      <div className="mt-5 border-t border-white/8 pt-4">
        <p className="mb-2 text-[11px] uppercase tracking-wide text-[var(--ink-faint)]">
          Or use a cloud engine
        </p>
        <CloudProviders />
      </div>
    </div>
  );
}

function ProgressRow({ label, value }: { label: string; value: number }) {
  return (
    <div className="space-y-1">
      <div className="flex justify-between text-[11px] text-[var(--ink-muted)]">
        <span>{label}</span>
        <span>{Math.round(value * 100)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-white/8">
        <div
          className="h-full rounded-full transition-[width]"
          style={{
            width: `${Math.round(value * 100)}%`,
            background: "linear-gradient(90deg, var(--aurora-1), var(--aurora-3))",
          }}
        />
      </div>
    </div>
  );
}

function PermissionsStep() {
  const [status, setStatus] = useState<boolean | null>(null);
  const [injected, setInjected] = useState(false);

  return (
    <div>
      <StepHeading
        title="Permissions & output"
        sub="Echo types transcripts into the focused app. Confirm it has permission and give it a try."
      />
      <div className="space-y-3">
        <div className="flex items-center justify-between rounded-xl border border-white/8 bg-white/[0.025] px-4 py-3">
          <span className="text-[12.5px] text-[var(--ink)]">Keyboard / accessibility access</span>
          <button
            onClick={async () => setStatus(await commands.checkAccessibilityPermission())}
            className="rounded-lg border border-white/12 px-2.5 py-1 text-[11px] text-[var(--ink)] transition hover:bg-white/8"
          >
            {status === null ? "Check" : status ? "Granted ✓" : "Not granted"}
          </button>
        </div>

        <div className="rounded-xl border border-white/8 bg-white/[0.025] px-4 py-3">
          <p className="mb-2 text-[12px] text-[var(--ink-muted)]">
            Click into the box, then press Test — Echo will type into it.
          </p>
          <div className="flex gap-2">
            <input
              placeholder="Focus me…"
              className="flex-1 rounded-lg border border-white/10 bg-white/5 px-2.5 py-1.5 text-[13px] text-[var(--ink)] outline-none focus:border-[var(--aurora-2)]/60"
            />
            <button
              onClick={() => {
                void commands.injectText("Hello from Echo ");
                setInjected(true);
              }}
              className="rounded-lg bg-[var(--aurora-2)] px-3 py-1.5 text-[12px] font-medium text-white transition hover:brightness-110"
            >
              Test
            </button>
          </div>
          {injected && (
            <p className="mt-2 text-[11px] text-emerald-300">
              Sent! If nothing appeared, grant the permission above.
            </p>
          )}
        </div>

        <p className="text-[10.5px] leading-snug text-[var(--ink-faint)]">
          macOS needs Accessibility permission (System Settings → Privacy). Linux needs{" "}
          <code>xdotool</code> or <code>ydotool</code>. Windows works out of the box.
        </p>
      </div>
    </div>
  );
}

function HotkeyStep() {
  const qc = useQueryClient();
  const { data: hotkey } = useQuery({ queryKey: ["hotkey"], queryFn: commands.getHotkey });

  return (
    <div>
      <StepHeading
        title="Your global shortcut"
        sub="Press this anywhere to start and stop dictation. You can change it later in Settings."
      />
      <HotkeyCapture
        value={hotkey ?? ""}
        onChange={(accel) =>
          void commands
            .registerHotkey(accel)
            .then(() => qc.invalidateQueries({ queryKey: ["hotkey"] }))
        }
      />
      <p className="mt-3 text-center text-[12px] text-[var(--ink-muted)]">
        You're all set — press <span className="text-[var(--ink)]">Finish</span> to start using Echo.
      </p>
    </div>
  );
}
