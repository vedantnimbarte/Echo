import Link from "next/link";
import Reveal from "@/components/ui/Reveal";
import Magnetic from "@/components/ui/Magnetic";
import CopyLine from "@/components/ui/CopyLine";
import { DOWNLOADS, INSTALL, LINKS, VERSION } from "@/lib/links";

const PLATFORMS = [
  { label: "macOS", note: "Apple Silicon · .dmg", href: DOWNLOADS.macos },
  { label: "Windows", note: "64-bit · .exe", href: DOWNLOADS.windows },
  { label: "Linux", note: "x86_64 · AppImage", href: DOWNLOADS.linuxAppImage },
];

export default function FinalCTA() {
  return (
    <section className="relative mx-auto mt-28 max-w-7xl px-6 sm:mt-40 sm:px-10">
      <div className="panel relative overflow-hidden rounded-[28px] px-6 py-16 sm:px-14 sm:py-20">
        <div
          className="measure-grid absolute inset-0 opacity-40"
          aria-hidden
        />

        <div className="relative mx-auto max-w-2xl text-center">
          <Reveal>
            <p className="eyebrow">v{VERSION} · MIT</p>
            <h2 className="mt-5 text-5xl sm:text-7xl">
              Stop <span className="glow-text">typing</span>.
            </h2>
            <p className="mx-auto mt-6 max-w-lg text-lg leading-relaxed text-fog">
              Free, open-source, and a few megabytes. No account to make, no
              trial to start, nothing to cancel.
            </p>
          </Reveal>

          <Reveal delay={0.08}>
            <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row">
              <Magnetic>
                <Link href="/download" className="btn-glow">
                  Download Echo
                </Link>
              </Magnetic>
              <a
                href={LINKS.github}
                target="_blank"
                rel="noreferrer"
                className="btn-ghost"
              >
                Read the source
              </a>
            </div>
          </Reveal>
        </div>

        <Reveal delay={0.14}>
          <div className="relative mx-auto mt-14 grid max-w-3xl gap-3 sm:grid-cols-2">
            <CopyLine label="macOS / Linux" command={INSTALL.unix} />
            <CopyLine label="Windows · PowerShell" command={INSTALL.windows} />
          </div>
        </Reveal>

        <Reveal delay={0.2}>
          <div className="relative mx-auto mt-3 grid max-w-3xl gap-3 sm:grid-cols-3">
            {PLATFORMS.map((p) => (
              <a
                key={p.label}
                href={p.href}
                className="glass lift flex items-center justify-between rounded-2xl px-4 py-3.5"
              >
                <span>
                  <span className="block text-sm font-semibold">{p.label}</span>
                  <span className="datum">{p.note}</span>
                </span>
                <svg
                  viewBox="0 0 20 20"
                  className="h-4 w-4 text-faint"
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
              </a>
            ))}
          </div>
        </Reveal>

        <Reveal delay={0.26}>
          <p className="relative mx-auto mt-8 max-w-xl text-center text-sm leading-relaxed text-faint">
            Echo isn&rsquo;t code-signed yet, so Windows and macOS will warn you
            on first launch.{" "}
            <Link href="/download" className="text-fog underline underline-offset-4 hover:text-glow">
              Here is exactly what you&rsquo;ll see
            </Link>
            .
          </p>
        </Reveal>
      </div>
    </section>
  );
}
