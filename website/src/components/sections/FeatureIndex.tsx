"use client";

import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import Reveal from "@/components/ui/Reveal";

const FEATURES = [
  {
    no: "01",
    title: "On-device transcription",
    tag: "Whisper · local",
    body: "Speech is transcribed by a Whisper model running on your own hardware. No request ever leaves the machine — there's nothing to intercept, log, or breach.",
  },
  {
    no: "02",
    title: "Types into anything",
    tag: "system injection",
    body: "Echo writes into whatever app has focus — Slack, your IDE, a browser field, a terminal. It speaks the OS's keyboard language, so every app just works.",
  },
  {
    no: "03",
    title: "The floating pill",
    tag: "always ready",
    body: "A small, frameless pill sits at the edge of your screen with a live waveform. One hotkey to start, one glance to know it's listening.",
  },
  {
    no: "04",
    title: "Hands-free auto mode",
    tag: "Silero VAD",
    body: "Arm it once and just talk. Voice-activity detection finds the edges of each utterance and transcribes segment by segment — no button holding.",
  },
  {
    no: "05",
    title: "Your words, your way",
    tag: "custom dictionary",
    body: "Teach Echo your names, jargon, and code symbols. A replacement pipeline fixes them in every transcript, automatically, before they hit the page.",
  },
  {
    no: "06",
    title: "Extensible by design",
    tag: "plugins · BYO key",
    body: "A native plugin system lets the community extend Echo. Prefer cloud accuracy? Bring your own OpenAI, Groq, or Deepgram key — it stays in your keychain.",
  },
];

export default function FeatureIndex() {
  const [active, setActive] = useState(0);

  return (
    <section className="mx-auto mt-24 max-w-7xl px-6 sm:px-10">
      <div className="grid gap-10 lg:grid-cols-[0.85fr_1.15fr]">
        <div className="lg:sticky lg:top-28 lg:self-start">
          <Reveal>
            <p className="eyebrow">What it does</p>
            <h2 className="mt-4 text-5xl font-semibold sm:text-6xl">
              Built like an
              <br />
              <span className="signal-gradient">instrument</span>.
            </h2>
            <p className="mt-5 max-w-md text-lg leading-relaxed text-fog">
              Not a toolbar with a microphone bolted on. Every part of Echo is
              designed around one motion: think it, say it, see it written.
            </p>
            <div className="panel mt-8 hidden rounded-card p-6 lg:block">
              <span className="eyebrow">selected</span>
              <p className="mt-2 font-display text-2xl text-text">
                {FEATURES[active].title}
              </p>
              <p className="mt-3 font-mono text-xs uppercase tracking-[0.2em] text-glow">
                {FEATURES[active].tag}
              </p>
            </div>
          </Reveal>
        </div>

        <ul className="flex flex-col">
          {FEATURES.map((f, i) => {
            const isActive = i === active;
            return (
              <li key={f.no}>
                <button
                  onMouseEnter={() => setActive(i)}
                  onFocus={() => setActive(i)}
                  onClick={() => setActive(i)}
                  className="group w-full border-t border-line py-6 text-left transition-colors last:border-b"
                >
                  <div className="flex items-baseline gap-5">
                    <span
                      className={`font-mono text-sm transition-colors ${
                        isActive ? "text-glow" : "text-faint"
                      }`}
                    >
                      {f.no}
                    </span>
                    <h3
                      className={`flex-1 text-2xl font-medium transition-all duration-300 sm:text-3xl ${
                        isActive ? "text-text translate-x-1" : "text-fog"
                      }`}
                    >
                      {f.title}
                    </h3>
                    <span
                      className={`mt-1 hidden shrink-0 font-mono text-[0.68rem] uppercase tracking-[0.2em] transition-opacity sm:block ${
                        isActive ? "text-glow opacity-100" : "opacity-0"
                      }`}
                    >
                      {f.tag}
                    </span>
                  </div>
                  <AnimatePresence initial={false}>
                    {isActive && (
                      <motion.div
                        initial={{ height: 0, opacity: 0 }}
                        animate={{ height: "auto", opacity: 1 }}
                        exit={{ height: 0, opacity: 0 }}
                        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
                        className="overflow-hidden"
                      >
                        <p className="max-w-xl pl-10 pt-4 text-base leading-relaxed text-fog">
                          {f.body}
                        </p>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </button>
              </li>
            );
          })}
        </ul>
      </div>
    </section>
  );
}
