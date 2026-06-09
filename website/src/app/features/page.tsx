import type { Metadata } from "next";
import PageHero from "@/components/site/PageHero";
import Reveal from "@/components/ui/Reveal";
import FinalCTA from "@/components/sections/FinalCTA";
import PillMock from "@/components/site/PillMock";

export const metadata: Metadata = {
  title: "Features — Echo",
  description:
    "On-device transcription, system-wide text injection, hands-free auto mode, custom dictionaries, and a plugin system. All local, all open.",
};

type Panel = {
  no: string;
  title: string;
  body: string;
  span: string;
  accent?: React.ReactNode;
};

const PANELS: Panel[] = [
  {
    no: "01",
    title: "On-device by default",
    body: "A Whisper model transcribes locally. Pick a size that fits your machine — tiny for instant, large for studio-grade accuracy.",
    span: "lg:col-span-4",
    accent: (
      <div className="mt-6 flex items-end gap-1.5">
        {[28, 44, 22, 52, 34, 60, 30, 48].map((h, i) => (
          <span
            key={i}
            className="w-2 rounded-full bg-gradient-to-t from-glow/30 to-glow anim-pulse"
            style={{ height: h, animationDelay: `${i * 0.12}s` }}
          />
        ))}
      </div>
    ),
  },
  {
    no: "02",
    title: "Works in every app",
    body: "Echo injects text through the OS keyboard layer, so it lands anywhere a cursor blinks.",
    span: "lg:col-span-2",
  },
  {
    no: "03",
    title: "Hands-free auto mode",
    body: "Silero voice-activity detection segments speech on the fly. Arm once and keep talking.",
    span: "lg:col-span-2",
  },
  {
    no: "04",
    title: "Custom dictionary",
    body: "Map mishearings to the right words — names, acronyms, code symbols — applied to every transcript automatically.",
    span: "lg:col-span-4",
    accent: (
      <div className="mt-6 space-y-2 font-mono text-xs">
        <div className="flex items-center gap-2 text-fog">
          <span className="text-faint">heard</span> &ldquo;rust lang&rdquo;
          <span className="text-glow">→</span>
          <span className="text-text">Rust</span>
        </div>
        <div className="flex items-center gap-2 text-fog">
          <span className="text-faint">heard</span> &ldquo;cloud airy&rdquo;
          <span className="text-glow">→</span>
          <span className="text-text">Cloudairy</span>
        </div>
      </div>
    ),
  },
  {
    no: "05",
    title: "Bring your own cloud",
    body: "Prefer hosted accuracy? Drop in an OpenAI, Groq, or Deepgram key — stored in your OS keychain, never on a server.",
    span: "lg:col-span-3",
  },
  {
    no: "06",
    title: "Extensible with plugins",
    body: "A native plugin contract lets the community add providers, post-processing, and integrations.",
    span: "lg:col-span-3",
  },
];

const STACK = [
  "Tauri",
  "Rust",
  "whisper.cpp",
  "Silero VAD",
  "CPAL audio",
  "SQLite",
  "React",
  "90+ languages",
];

export default function FeaturesPage() {
  return (
    <>
      <PageHero
        eyebrow="Features"
        title="Everything happens"
        highlight="on your machine"
        subtitle="Echo is a complete dictation engine — capture, detection, transcription, correction, and injection — with nothing routed through the cloud unless you ask for it."
      />

      <section className="mx-auto mt-16 max-w-7xl px-6 sm:px-10">
        <div className="grid gap-4 lg:grid-cols-6">
          {PANELS.map((p, i) => (
            <Reveal key={p.no} delay={(i % 3) * 0.08} className={p.span}>
              <article className="panel group h-full rounded-card p-6 transition-colors hover:border-line-2">
                <div className="flex items-center justify-between">
                  <span className="font-mono text-sm text-glow">{p.no}</span>
                  <span className="h-1.5 w-1.5 rounded-full bg-line-2 transition-colors group-hover:bg-glow" />
                </div>
                <h3 className="mt-5 text-2xl font-medium">{p.title}</h3>
                <p className="mt-3 max-w-md leading-relaxed text-fog">{p.body}</p>
                {p.accent}
              </article>
            </Reveal>
          ))}
        </div>
      </section>

      {/* under the hood */}
      <section className="mx-auto mt-20 max-w-7xl px-6 sm:px-10">
        <div className="panel relative overflow-hidden rounded-3xl p-7 sm:p-12">
          <div className="grid items-center gap-10 lg:grid-cols-[1fr_1fr]">
            <Reveal>
              <p className="eyebrow">Under the hood</p>
              <h2 className="mt-5 text-4xl font-semibold sm:text-5xl">
                A tiny native app,
                <br />
                not a browser tab.
              </h2>
              <p className="mt-6 max-w-md text-lg leading-relaxed text-fog">
                Echo is built on Tauri and Rust — a few megabytes, near-zero idle
                cost, and direct access to your audio and keyboard. The kind of
                software that feels instant because it is.
              </p>
              <div className="mt-8 flex flex-wrap gap-2.5">
                {STACK.map((s) => (
                  <span
                    key={s}
                    className="rounded-full border border-line-2 px-3.5 py-1.5 font-mono text-xs text-fog"
                  >
                    {s}
                  </span>
                ))}
              </div>
            </Reveal>
            <Reveal delay={0.12}>
              <div className="flex flex-col items-center gap-6">
                <PillMock />
                <PillMock state="done" />
                <p className="font-mono text-[0.66rem] uppercase tracking-[0.2em] text-faint">
                  the only UI you&apos;ll see
                </p>
              </div>
            </Reveal>
          </div>
        </div>
      </section>

      <FinalCTA />
    </>
  );
}
