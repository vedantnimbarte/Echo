import Reveal from "@/components/ui/Reveal";
import WaveSignal from "@/components/ui/WaveSignal";

const POINTS = [
  { k: "no servers", v: "Audio is processed by a model on your own CPU/GPU. There is no backend to send it to." },
  { k: "no telemetry", v: "Usage stays local. Optional analytics are stored on disk and never transmitted." },
  { k: "no account", v: "Download and dictate. There's no sign-up, no email, no license server." },
];

export default function Privacy() {
  return (
    <section className="relative mx-auto mt-24 max-w-7xl px-6 sm:px-10">
      <div className="panel relative overflow-hidden rounded-3xl p-7 sm:p-12">
        <div className="scanline" aria-hidden />
        <div className="grid items-center gap-10 lg:grid-cols-[1.1fr_0.9fr]">
          <Reveal>
            <p className="eyebrow">Privacy isn&apos;t a setting</p>
            <h2 className="mt-4 text-5xl font-semibold leading-[0.95] sm:text-7xl">
              Your voice never
              <br />
              leaves <span className="signal-gradient">this device</span>.
            </h2>
            <p className="mt-6 max-w-lg text-lg leading-relaxed text-fog">
              Most dictation tools stream your microphone to someone else&apos;s
              computer. Echo doesn&apos;t. The model lives with you, so what you
              say stays exactly where you said it.
            </p>

            <dl className="mt-8 grid gap-px overflow-hidden rounded-2xl border border-line sm:grid-cols-3">
              {POINTS.map((p) => (
                <div key={p.k} className="bg-ink-1/60 p-5">
                  <dt className="font-mono text-xs uppercase tracking-[0.18em] text-glow">
                    {p.k}
                  </dt>
                  <dd className="mt-3 text-sm leading-relaxed text-fog">{p.v}</dd>
                </div>
              ))}
            </dl>
          </Reveal>

          {/* contained "this machine" boundary */}
          <Reveal delay={0.15}>
            <div className="relative">
              <div className="rounded-3xl border border-dashed border-line-2 p-5">
                <div className="flex items-center justify-between font-mono text-[0.66rem] uppercase tracking-[0.2em] text-faint">
                  <span>localhost</span>
                  <span className="text-glow">● secure</span>
                </div>
                <div className="mt-2 overflow-hidden rounded-2xl bg-ink/60">
                  <WaveSignal height={180} lines={4} intensity={0.85} speed={0.8} />
                </div>
                <p className="mt-3 text-center font-mono text-[0.66rem] uppercase tracking-[0.2em] text-faint">
                  audio in · text out · nothing outbound
                </p>
              </div>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
