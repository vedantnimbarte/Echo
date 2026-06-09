import type { Metadata } from "next";
import Link from "next/link";
import PageHero from "@/components/site/PageHero";
import Reveal from "@/components/ui/Reveal";
import Magnetic from "@/components/ui/Magnetic";
import FinalCTA from "@/components/sections/FinalCTA";

export const metadata: Metadata = {
  title: "Pricing — Echo",
  description:
    "Echo is free and open-source. Run it entirely on-device at zero cost, or bring your own cloud key and pay your provider directly.",
};

const INCLUDED = [
  "Unlimited dictation",
  "Every local Whisper model",
  "Works in every app",
  "Hands-free auto mode",
  "Custom dictionary",
  "Plugin system",
  "No account, ever",
  "MIT licensed source",
];

const MODES = [
  {
    name: "Local",
    tag: "default · recommended",
    price: "$0",
    note: "Everything runs on your hardware.",
    points: ["Fully private & offline", "No API keys", "No usage limits", "You pay nothing to anyone"],
    glow: true,
  },
  {
    name: "Cloud (optional)",
    tag: "bring your own key",
    price: "you pay your provider",
    note: "Route to OpenAI, Groq, or Deepgram if you want.",
    points: ["Your key, your bill", "Stored in your keychain", "Echo adds zero markup", "Switch back anytime"],
    glow: false,
  },
];

const FAQ = [
  {
    q: "Is it actually free?",
    a: "Yes. Echo is open-source under the MIT license. The local experience costs nothing and never will — there's no paywall, trial, or upsell.",
  },
  {
    q: "Then how is Echo funded?",
    a: "It's a community-driven, open-source project. There are no servers to run for local use, so there's almost nothing to fund. Contributions are welcome on GitHub.",
  },
  {
    q: "Do I need an internet connection?",
    a: "No. Local transcription works fully offline. You only need a connection if you opt into a cloud provider with your own key.",
  },
  {
    q: "What does 'bring your own key' cost?",
    a: "Whatever your chosen provider charges — you're billed by them directly. Echo never sees your key beyond storing it in your OS keychain.",
  },
];

export default function PricingPage() {
  return (
    <>
      <PageHero
        eyebrow="Pricing"
        title="It's free."
        highlight="Genuinely."
        subtitle="Echo is open-source and local-first. The whole app is yours at no cost — pricing only enters the picture if you choose to use a paid cloud provider with your own key."
      />

      {/* hero plan */}
      <section className="mx-auto mt-14 max-w-3xl px-6 sm:px-10">
        <Reveal>
          <div className="panel glow-ring relative overflow-hidden rounded-3xl p-8 text-center sm:p-12">
            <div className="scanline" aria-hidden />
            <p className="eyebrow">The whole app</p>
            <div className="mt-6 flex items-end justify-center gap-3">
              <span className="font-display text-8xl font-bold leading-none signal-gradient">
                $0
              </span>
              <span className="mb-2 text-lg text-fog">/ forever</span>
            </div>
            <p className="mx-auto mt-4 max-w-md text-fog">
              No tiers. No seats. No asterisks. Download it and dictate.
            </p>

            <ul className="mx-auto mt-9 grid max-w-lg grid-cols-1 gap-x-8 gap-y-3 text-left sm:grid-cols-2">
              {INCLUDED.map((f) => (
                <li key={f} className="flex items-center gap-3 text-text">
                  <span className="flex h-5 w-5 items-center justify-center rounded-full bg-glow/15 text-glow">
                    <svg viewBox="0 0 12 12" className="h-3 w-3" fill="none" stroke="currentColor" strokeWidth={2}>
                      <path d="M2.5 6.5 5 9l4.5-5.5" strokeLinecap="round" strokeLinejoin="round" />
                    </svg>
                  </span>
                  {f}
                </li>
              ))}
            </ul>

            <div className="mt-10 flex justify-center">
              <Magnetic>
                <Link href="/download" className="btn-glow text-base">
                  Download Echo
                </Link>
              </Magnetic>
            </div>
          </div>
        </Reveal>
      </section>

      {/* local vs cloud */}
      <section className="mx-auto mt-10 max-w-5xl px-6 sm:px-10">
        <div className="grid gap-4 md:grid-cols-2">
          {MODES.map((m, i) => (
            <Reveal key={m.name} delay={i * 0.1}>
              <div
                className={`h-full rounded-card p-6 ${
                  m.glow ? "panel glow-ring" : "panel"
                }`}
              >
                <div className="flex items-center justify-between">
                  <h3 className="text-2xl font-medium">{m.name}</h3>
                  <span className="font-mono text-[0.66rem] uppercase tracking-[0.18em] text-glow">
                    {m.tag}
                  </span>
                </div>
                <p className="mt-4 font-display text-3xl text-text">{m.price}</p>
                <p className="mt-2 text-sm text-fog">{m.note}</p>
                <ul className="mt-6 space-y-2.5 text-sm text-fog">
                  {m.points.map((p) => (
                    <li key={p} className="flex items-center gap-2.5">
                      <span className="h-1 w-1 rounded-full bg-glow" />
                      {p}
                    </li>
                  ))}
                </ul>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      {/* faq */}
      <section className="mx-auto mt-20 max-w-3xl px-6 sm:px-10">
        <Reveal>
          <h2 className="text-center text-4xl font-semibold sm:text-5xl">
            Questions, answered
          </h2>
        </Reveal>
        <div className="mt-10 divide-y divide-line border-y border-line">
          {FAQ.map((item, i) => (
            <Reveal key={item.q} delay={i * 0.05}>
              <details className="group py-5">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-4 text-lg font-medium text-text">
                  {item.q}
                  <span className="font-mono text-glow transition-transform duration-300 group-open:rotate-45">
                    +
                  </span>
                </summary>
                <p className="mt-4 max-w-2xl leading-relaxed text-fog">{item.a}</p>
              </details>
            </Reveal>
          ))}
        </div>
      </section>

      <FinalCTA />
    </>
  );
}
