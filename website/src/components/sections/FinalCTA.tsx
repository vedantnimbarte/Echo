import Link from "next/link";
import Reveal from "@/components/ui/Reveal";
import WaveSignal from "@/components/ui/WaveSignal";
import Magnetic from "@/components/ui/Magnetic";
import { LINKS } from "@/lib/links";

export default function FinalCTA() {
  return (
    <section className="relative mx-auto mt-24 max-w-5xl px-6 text-center sm:px-10">
      <div className="pointer-events-none absolute inset-x-0 -top-10 -z-10 opacity-50">
        <WaveSignal height={260} lines={6} intensity={1.1} />
      </div>
      <Reveal>
        <p className="eyebrow">Free forever · open-source</p>
        <h2 className="mx-auto mt-5 max-w-3xl text-balance text-6xl font-semibold leading-[0.95] sm:text-8xl">
          Stop typing.
          <br />
          Start <span className="signal-gradient">talking</span>.
        </h2>
        <p className="mx-auto mt-6 max-w-xl text-lg text-fog">
          Install Echo in under a minute. Your next sentence could be spoken.
        </p>
        <div className="mt-8 flex flex-col items-center justify-center gap-4 sm:flex-row">
          <Magnetic>
            <Link href="/download" className="btn-glow text-base">
              Download Echo
              <svg viewBox="0 0 20 20" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth={2}>
                <path d="M10 3v10m0 0 4-4m-4 4-4-4M4 17h12" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </Link>
          </Magnetic>
          <a href={LINKS.github} target="_blank" rel="noreferrer" className="btn-ghost text-base">
            Star on GitHub
          </a>
        </div>
      </Reveal>
    </section>
  );
}
