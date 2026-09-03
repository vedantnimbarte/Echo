"use client";

import Link from "next/link";
import dynamic from "next/dynamic";
import { motion } from "motion/react";
import Magnetic from "@/components/ui/Magnetic";
import DetectionMeter from "@/components/three/DetectionMeter";
import { VERSION } from "@/lib/links";

const MelTerrain = dynamic(() => import("@/components/three/MelTerrain"), {
  ssr: false,
});

const EASE = [0.16, 1, 0.3, 1] as const;
const up = (d: number) => ({
  initial: { opacity: 0, y: 16 },
  animate: { opacity: 1, y: 0 },
  transition: { duration: 0.75, delay: d, ease: EASE },
});

export default function Hero() {
  return (
    <section className="relative isolate overflow-hidden px-6 pb-20 pt-32 sm:px-10 sm:pt-40">
      <div className="floor-grid -z-20" aria-hidden />

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 1.6, delay: 0.2, ease: "easeOut" }}
        className="pointer-events-none absolute inset-x-0 bottom-0 -z-10 h-[64vh] min-h-[400px]"
        aria-hidden
      >
        <MelTerrain className="h-full w-full" />
      </motion.div>

      <div className="mx-auto max-w-4xl text-center">
        <motion.div {...up(0)} className="flex justify-center">
          <span className="glass inline-flex items-center gap-2.5 rounded-full px-4 py-1.5">
            <span className="h-1.5 w-1.5 rounded-full bg-glow anim-pulse" />
            <span className="datum uppercase tracking-[0.22em]">
              wake word · runs on your machine
            </span>
          </span>
        </motion.div>

        <motion.h1
          {...up(0.08)}
          className="mt-8 text-balance text-5xl leading-[0.92] font-semibold sm:text-7xl lg:text-8xl"
        >
          Say the word.
          <br />
          Start <span className="glow-text">talking</span>.
        </motion.h1>

        <motion.p
          {...up(0.16)}
          className="mx-auto mt-7 max-w-xl text-balance text-lg leading-relaxed text-fog sm:text-xl"
        >
          Echo waits for a phrase you choose, then types what you say into
          whatever app is focused. The listening, the model, and the transcript
          never leave your machine.
        </motion.p>

        <motion.div
          {...up(0.24)}
          className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row"
        >
          <Magnetic>
            <Link href="/download" className="btn-glow">
              Download for free
              <svg
                viewBox="0 0 20 20"
                className="h-4 w-4"
                fill="none"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  d="M10 3v10m0 0 4-4m-4 4-4-4M4 17h12"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
              </svg>
            </Link>
          </Magnetic>
          <Link href="#pipeline" className="btn-ghost">
            See the signal path
          </Link>
        </motion.div>

        <motion.p {...up(0.32)} className="datum mt-6">
          macOS · Windows · Linux — v{VERSION}, MIT licensed
        </motion.p>
      </div>

      {/* HUD: the classifier's own readout, over the surface it reads from */}
      <motion.div
        {...up(0.44)}
        className="relative mx-auto mt-14 flex max-w-7xl flex-col items-center gap-6 sm:mt-20 lg:flex-row lg:items-end lg:justify-between"
      >
        <DetectionMeter />
        <p className="datum hidden text-right leading-relaxed lg:block">
          above: a mel spectrogram, extruded
          <br />
          32 bins × 76 frames — the wake model&rsquo;s real input
        </p>
      </motion.div>
    </section>
  );
}
