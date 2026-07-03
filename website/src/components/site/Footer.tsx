import Link from "next/link";
import { EchoMark } from "./Logo";
import { LINKS } from "@/lib/links";

const COLS: { title: string; links: { label: string; href: string }[] }[] = [
  {
    title: "Product",
    links: [
      { label: "Features", href: "/features" },
      { label: "Pricing", href: "/pricing" },
      { label: "Download", href: "/download" },
      { label: "Changelog", href: "/download" },
    ],
  },
  {
    title: "Open source",
    links: [
      { label: "GitHub", href: LINKS.github },
      { label: "Contributing", href: LINKS.contributing },
      { label: "Plugins", href: LINKS.plugins },
      { label: "License", href: LINKS.license },
    ],
  },
  {
    title: "Company",
    links: [
      { label: "Privacy", href: "/" },
      { label: "Manifesto", href: "/" },
      { label: "Contact", href: "/" },
    ],
  },
];

export default function Footer() {
  return (
    <footer className="relative mt-24 overflow-hidden border-t border-line">
      <div className="mx-auto max-w-7xl px-6 pb-12 pt-16 sm:px-10">
        <div className="grid gap-12 lg:grid-cols-[1.4fr_2fr]">
          <div>
            <div className="flex items-center gap-3 text-glow">
              <EchoMark className="h-7 w-24" />
              <span className="font-display text-2xl font-semibold text-text">
                Echo
              </span>
            </div>
            <p className="mt-6 max-w-sm text-lg leading-relaxed text-fog">
              Your voice never leaves your device. On-device dictation for every
              app — open, private, and fast.
            </p>
            <p className="eyebrow mt-8">Local-first · Open-source · MIT</p>
          </div>

          <div className="grid grid-cols-2 gap-8 sm:grid-cols-3">
            {COLS.map((col) => (
              <div key={col.title}>
                <h4 className="font-mono text-[0.7rem] uppercase tracking-[0.25em] text-faint">
                  {col.title}
                </h4>
                <ul className="mt-5 space-y-3">
                  {col.links.map((l) => (
                    <li key={l.label}>
                      <Link
                        href={l.href}
                        className="text-fog transition-colors hover:text-glow"
                      >
                        {l.label}
                      </Link>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </div>

        <div className="mt-12 flex flex-col items-start justify-between gap-4 border-t border-line pt-8 text-sm text-faint sm:flex-row sm:items-center">
          <p>© {new Date().getFullYear()} Echo. Released under the MIT license.</p>
          <p className="font-mono text-[0.72rem] tracking-wide">
            <span className="text-glow anim-pulse">●</span> all processing stays
            on this machine
          </p>
        </div>
      </div>

      {/* oversized ghost wordmark */}
      <div
        aria-hidden
        className="pointer-events-none select-none px-6 text-center font-display text-[22vw] font-bold leading-none tracking-tighter text-text/[0.025] sm:px-10"
      >
        ECHO
      </div>
    </footer>
  );
}
