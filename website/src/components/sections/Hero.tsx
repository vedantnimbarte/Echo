"use client";

import Link from "next/link";
import { motion } from "motion/react";
import WaveSignal from "@/components/ui/WaveSignal";
import TypeText from "@/components/ui/TypeText";
import PillMock from "@/components/site/PillMock";
import Magnetic from "@/components/ui/Magnetic";

const EASE = [0.22, 1, 0.36, 1] as const;
const up = (d: number) => ({
  initial: { opacity: 0, y: 14 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.5, delay: d, ease: EASE },
});

export default function Hero() {
  return (
    <section className="relative px-6 pt-32 sm:px-10 sm:pt-40">
      {/* faint measurement grid backdrop */}
      <div className="measure-grid absolute inset-0 -z-10 opacity-50" aria-hidden />

      <div className="mx-auto max-w-5xl text-center">
        <motion.div {...up(0)} className="flex justify-center">
          <span className="glass inline-flex items-center gap-2 rounded-full px-4 py-1.5 eyebrow">
            <span className="h-1.5 w-1.5 rounded-full bg-glow anim-pulse" />
            On-device voice dictation
          </span>
        </motion.div>

        <motion.h1
          {...up(0.06)}
          className="mt-7 text-balance text-6xl font-semibold leading-[0.92] sm:text-8xl"
        >
          Speak.
          <br />
          It&apos;s already <span className="signal-gradient">typed</span>.
        </motion.h1>

        <motion.p
          {...up(0.12)}
          className="mx-auto mt-6 max-w-2xl text-balance text-lg leading-relaxed text-fog sm:text-xl"
        >
          Echo turns your voice into text inside any app — a chat box, an email,
          your editor. It runs entirely on your machine. No cloud, no accounts,
          no trace.
        </motion.p>

        <motion.div
          {...up(0.18)}
          className="mt-9 flex flex-col items-center justify-center gap-4 sm:flex-row"
        >
          <Magnetic>
            <Link href="/download" className="btn-glow">
              Download for free
              <svg viewBox="0 0 20 20" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2}>
                <path d="M10 3v10m0 0 4-4m-4 4-4-4M4 17h12" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </Link>
          </Magnetic>
          <Link href="/features" className="btn-ghost">
            See how it works
          </Link>
        </motion.div>

        <motion.p {...up(0.24)} className="mt-5 font-mono text-xs tracking-wide text-faint">
          macOS · Windows · Linux — open-source &amp; MIT licensed
        </motion.p>
      </div>

      {/* signal band */}
      <motion.div
        initial={{ opacity: 0, scale: 0.98 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.7, delay: 0.26, ease: EASE }}
        className="relative mx-auto mt-12 max-w-6xl"
      >
        <div className="pointer-events-none absolute -left-2 top-1/2 hidden -translate-y-1/2 font-mono text-[0.65rem] uppercase tracking-[0.3em] text-faint sm:block">
          voice
        </div>
        <div className="pointer-events-none absolute -right-2 top-1/2 hidden -translate-y-1/2 font-mono text-[0.65rem] uppercase tracking-[0.3em] text-faint sm:block">
          text
        </div>

        <WaveSignal height={300} lines={5} className="cursor-crosshair" />

        {/* output line that types under the signal */}
        <div className="mx-auto -mt-6 flex max-w-2xl items-center justify-center">
          <div className="panel rounded-2xl px-5 py-3 text-left">
            <span className="font-mono text-[0.65rem] uppercase tracking-[0.25em] text-faint">
              output
            </span>
            <p className="mt-1 text-base text-text sm:text-lg">
              <TypeText
                phrases={[
                  "Reschedule the sync to Thursday afternoon.",
                  "Add bullet points and tighten the intro.",
                  "git commit -m \"fix: debounce the resize handler\"",
                ]}
              />
            </p>
          </div>
        </div>

        <PillMock className="absolute -bottom-6 left-1/2 -translate-x-1/2 anim-float" />
      </motion.div>
    </section>
  );
}
