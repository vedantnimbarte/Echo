import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import {
  Mic,
  ShieldCheck,
  Download,
  Keyboard,
  Radio,
  Sparkles,
  Check,
  Loader2,
  ArrowRight,
  ArrowLeft,
} from "lucide-react";

import { commands, type InputTest } from "../../ipc/commands";
import { echoEvents } from "../../ipc/events";
import { Waveform } from "../pill/Waveform";
import { CloudProviders } from "../settings/CloudProviders";
import { WakeWordSettings } from "../settings/WakeWordSettings";
import { HotkeyCapture } from "../common/HotkeyCapture";
import { Hint } from "../common/Hint";
import { TitleBar } from "../common/TitleBar";

type StepId = "welcome" | "mic" | "engine" | "permissions" | "hotkey" | "wake";
const STEPS: { id: StepId; label: string; Icon: React.ElementType }[] = [
  { id: "welcome", label: "Welcome", Icon: Sparkles },
  { id: "mic", label: "Microphone", Icon: Mic },
  { id: "engine", label: "Transcription", Icon: Download },
  { id: "permissions", label: "Permissions", Icon: ShieldCheck },
  { id: "hotkey", label: "Shortcut", Icon: Keyboard },
  { id: "wake", label: "Wake word", Icon: Radio },
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
    <div className="relative flex h-screen flex-col overflow-hidden bg-[var(--surface-0)] text-[var(--ink)]">
      {/* The same ambient top light as the settings window, so arriving in one
          from the other doesn't feel like changing apps. */}
      <div
        className="pointer-events-none absolute inset-x-0 top-0 h-64"
        style={{
          background:
            "radial-gradient(75% 100% at 50% 0%, rgba(255,240,224,0.055), transparent 70%)",
        }}
      />

      {/* Step rail */}
      <TitleBar />

      <div className="relative flex items-center justify-center gap-2 pt-5">
        {STEPS.map((s, i) => (
          <div key={s.id} className="flex items-center gap-2">
            <span
              className={clsx(
                "flex h-7 w-7 items-center justify-center rounded-full border text-[13px] transition",
                i < stepIdx && "border-[var(--hairline-strong)] bg-[var(--surface-3)] text-[var(--ink)]",
                i === stepIdx && "border-[var(--ink)] bg-[var(--surface-3)] text-[var(--ink)]",
                i > stepIdx && "border-[var(--hairline)] text-[var(--ink-faint)]"
              )}
            >
              {i < stepIdx ? <Check className="h-3.5 w-3.5" /> : <s.Icon className="h-3.5 w-3.5" />}
            </span>
            {i < STEPS.length - 1 && (
              <span
                className={clsx(
                  "h-px w-7 transition",
                  i < stepIdx ? "bg-[var(--ink)]/45" : "bg-[var(--surface-3)]"
                )}
              />
            )}
          </div>
        ))}
      </div>

      {/* Step body */}
      <div className="relative flex min-h-0 flex-1 items-center justify-center px-6">
        <div className="w-full max-w-[460px]">
          {step === "welcome" && <WelcomeStep />}
          {step === "mic" && <MicStep />}
          {step === "engine" && <EngineStep />}
          {step === "permissions" && <PermissionsStep />}
          {step === "hotkey" && <HotkeyStep />}
          {step === "wake" && <WakeStep />}
        </div>
      </div>

      {/* Nav */}
      <div className="relative flex items-center justify-between border-t border-[var(--hairline)] px-6 py-4">
        <button
          onClick={back}
          disabled={stepIdx === 0}
          className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[14px] text-[var(--ink-muted)] transition hover:text-[var(--ink)] disabled:opacity-0"
        >
          <ArrowLeft className="h-3.5 w-3.5" /> Back
        </button>
        <button
          onClick={finish}
          className="text-[13px] text-[var(--ink-faint)] transition hover:text-[var(--ink-muted)]"
        >
          Skip setup
        </button>
        <button
          onClick={isLast ? finish : next}
          className="btn-primary px-4 py-1.5 text-[14px]"
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
    <div className="mb-7 text-center">
      <h2 className="display text-[32px]">{title}</h2>
      <p className="mx-auto mt-2.5 max-w-[380px] text-[15px] leading-relaxed text-[var(--ink-muted)]">
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
          background:
            "linear-gradient(140deg, rgba(255,246,235,0.16), rgba(255,246,235,0.04))",
          boxShadow: "0 12px 40px -8px rgba(0,0,0,0.6)",
        }}
      >
        <Sparkles className="h-7 w-7 text-white" />
      </div>
      <StepHeading
        title="Welcome to Echo"
        sub="Your voice, typed into any app. A few quick steps and it's yours."
      />
      <div className="mx-auto max-w-[360px] space-y-2 text-left">
        {[
          "Private by default — local Whisper, offline",
          "Works in any app via your global shortcut",
          "Optional cloud engines for speed & accuracy",
        ].map((t) => (
          <div key={t} className="flex items-center gap-2.5 text-[14.5px] text-[var(--ink-muted)]">
            <Check className="h-4 w-4 shrink-0 text-[var(--ink)]" />
            {t}
          </div>
        ))}
      </div>
    </div>
  );
}

function MicStep() {
  const [testing, setTesting] = useState(false);
  // What the last test measured. The meter beside it shows audio *after* the
  // capture gain, so it looks lively even on an input that is barely working —
  // this is the number that tells the truth about the device.
  const [result, setResult] = useState<InputTest | null>(null);
  const { data: devices = [] } = useQuery({
    queryKey: ["audio-devices"],
    queryFn: commands.getAudioDevices,
  });
  const { data: savedDevice } = useQuery({
    queryKey: ["setting", "audio_device"],
    queryFn: () => commands.getSetting("audio_device"),
  });
  const qc = useQueryClient();

  // Listen for a few seconds, then say what was heard. The capture itself
  // measures the raw device level; nothing is transcribed and nothing is typed.
  async function runTest() {
    setTesting(true);
    setResult(null);
    try {
      setResult(await commands.testInputLevel(savedDevice ?? undefined));
    } catch {
      setResult(null);
    } finally {
      setTesting(false);
    }
  }

  return (
    <div>
      <StepHeading title="Pick your microphone" sub="Choose an input and test that Echo hears you." />
      <div className="space-y-3">
        <select
          className="field py-2"
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

        <div className="flex items-center justify-between rounded-xl glass px-4 py-3">
          <div className="flex h-6 items-center">
            {testing ? (
              <Waveform mode="listening" />
            ) : (
              <span className="text-[14px] text-[var(--ink-faint)]">
                {result ? `Peak ${result.peak_dbfs.toFixed(0)} dB` : "Meter idle"}
              </span>
            )}
          </div>
          <button
            onClick={() => void runTest()}
            disabled={testing}
            className={clsx(
              "rounded-lg px-3 py-1.5 text-[14px] font-medium transition",
              testing
                ? "bg-[var(--rec)] text-white"
                : "border border-[var(--hairline)] text-[var(--ink)] hover:bg-[var(--surface-2)]"
            )}
          >
            {testing ? "Listening…" : "Test microphone"}
          </button>
        </div>

        {testing && (
          <p className="text-[13px] text-[var(--ink-muted)]">
            Say something at your normal volume…
          </p>
        )}
        {result && !testing && (
          <p
            className={clsx(
              "text-[13px]",
              result.verdict === "good"
                ? "text-[var(--ink-muted)]"
                : result.verdict === "silent"
                  ? "text-[var(--rec)]"
                  : "text-[var(--ink)]"
            )}
          >
            {result.advice}
          </p>
        )}
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
        sub="Echo runs Whisper on your machine. This downloads a small English model, once."
      />

      {ready ? (
        <div className="flex items-center gap-2.5 rounded-xl border border-[var(--hairline-strong)] bg-[var(--surface-2)] px-4 py-3 text-[15px] text-[var(--ink)]">
          <Check className="h-4 w-4" /> Local transcription is ready.
        </div>
      ) : (
        <div className="space-y-3">
          <button
            onClick={provision}
            disabled={busy}
            className="btn-primary w-full rounded-xl px-4 py-2.5 text-[15px]"
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
          {note && <p className="text-[13px] leading-snug text-[var(--ink-muted)]">{note}</p>}
        </div>
      )}

      <div className="mt-5 border-t border-[var(--hairline)] pt-4">
        <p className="mb-3 text-[14px] font-medium text-[var(--ink-muted)]">
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
      <div className="flex justify-between text-[13px] text-[var(--ink-muted)]">
        <span>{label}</span>
        <span>{Math.round(value * 100)}%</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full bg-[var(--surface-2)]">
        <div
          className="h-full rounded-full transition-[width]"
          style={{
            width: `${Math.round(value * 100)}%`,
            background: "linear-gradient(90deg, var(--ink-muted), var(--ink))",
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
        <div className="flex items-center justify-between rounded-xl glass px-4 py-3">
          <span className="text-[14.5px] text-[var(--ink)]">Keyboard / accessibility access</span>
          <button
            onClick={async () => setStatus(await commands.checkAccessibilityPermission())}
            className="btn-ghost px-2.5 py-1 text-[13px]"
          >
            {status === null ? "Check" : status ? "Granted ✓" : "Not granted"}
          </button>
        </div>

        <div className="rounded-xl glass px-4 py-3">
          <p className="mb-2 text-[14px] text-[var(--ink-muted)]">
            Click into the box, then press Test — Echo will type into it.
          </p>
          <div className="flex gap-2">
            <input
              placeholder="Focus me…"
              className="field flex-1"
            />
            <button
              onClick={() => {
                void commands.injectText("Hello from Echo ");
                setInjected(true);
              }}
              className="btn-primary px-3 py-1.5 text-[14px]"
            >
              Test
            </button>
          </div>
          {injected && (
            <p className="mt-2 text-[13px] text-[var(--ink)]">
              Sent! If nothing appeared, grant the permission above.
            </p>
          )}
        </div>

        <div className="flex items-center gap-1.5 text-[13.5px] text-[var(--ink-faint)]">
          Requirements differ by platform
          <Hint label="Platform requirements">
            macOS needs Accessibility permission, under System Settings → Privacy.
            Linux needs <code>xdotool</code> or <code>ydotool</code> installed.
            Windows works out of the box.
          </Hint>
        </div>
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
      <p className="mt-3 text-center text-[14px] text-[var(--ink-muted)]">
        That's the essentials. One optional extra on the next step.
      </p>
    </div>
  );
}

function WakeStep() {
  return (
    <div>
      <StepHeading
        title="Start by voice (optional)"
        sub="Turn this on and Echo starts listening when you say a phrase, so you never have to reach for the keyboard."
      />
      <WakeWordSettings />
      <div className="mt-4 flex items-center justify-center gap-1.5 text-[13.5px] text-[var(--ink-faint)]">
        Safe to skip
        <Hint label="About skipping the wake word">
          Leaving this off keeps the microphone closed until you press your
          shortcut — the shortcut alone is a complete setup. You can turn a wake
          word on at any time in Settings, under Dictation.
        </Hint>
      </div>
    </div>
  );
}
