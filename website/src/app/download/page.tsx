import type { Metadata } from "next";
import PageHero from "@/components/site/PageHero";
import Reveal from "@/components/ui/Reveal";
import Magnetic from "@/components/ui/Magnetic";
import { DOWNLOADS, INSTALL, LINKS, VERSION } from "@/lib/links";

export const metadata: Metadata = {
  title: "Download — Echo",
  description:
    "Download Echo for macOS (Apple Silicon), Windows, and Linux. Free, open-source, and a few megabytes.",
};

const ICONS: Record<string, React.ReactNode> = {
  mac: (
    <svg viewBox="0 0 24 24" className="h-7 w-7 fill-current" aria-hidden>
      <path d="M16.4 12.6c0-2.3 1.9-3.4 2-3.5-1.1-1.6-2.8-1.8-3.4-1.8-1.5-.2-2.8.8-3.5.8s-1.9-.8-3.1-.8c-1.6 0-3 .9-3.8 2.4-1.6 2.8-.4 7 1.2 9.3.8 1.1 1.7 2.4 2.9 2.3 1.2 0 1.6-.7 3-.7s1.8.7 3 .7 2-1 2.8-2.1c.9-1.3 1.2-2.5 1.3-2.6-.1 0-2.4-1-2.4-3.7ZM14 5.7c.7-.8 1.1-1.9 1-3-.9.1-2 .6-2.7 1.4-.6.7-1.1 1.8-1 2.9 1 .1 2-.5 2.7-1.3Z" />
    </svg>
  ),
  win: (
    <svg viewBox="0 0 24 24" className="h-7 w-7 fill-current" aria-hidden>
      <path d="M3 4.5 10.5 3.4V11H3V4.5Zm0 15L10.5 20.6V13H3v6.5ZM11.5 3.2 21 1.8V11h-9.5V3.2Zm0 17.6L21 22.2V13h-9.5v7.8Z" />
    </svg>
  ),
  linux: (
    <svg viewBox="0 0 24 24" className="h-7 w-7 fill-current" aria-hidden>
      <path d="M12 2c-2 0-3.2 1.7-3.2 3.8 0 1.3.4 2 .4 3.1 0 1.1-1.2 2.1-2 3.7-.8 1.5-1.7 3.2-1.7 4.8 0 .9.4 1.5 1 1.9-.1.5 0 1 .4 1.4.7.6 2 .8 3.3.8s2.6-.2 3.3-.8c.4-.4.5-.9.4-1.4.6-.4 1-1 1-1.9 0-1.6-.9-3.3-1.7-4.8-.8-1.6-2-2.6-2-3.7 0-1.1.4-1.8.4-3.1C15.2 3.7 14 2 12 2Zm-1.4 4.1c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9Zm2.8 0c.4 0 .7.4.7.9s-.3.9-.7.9-.7-.4-.7-.9.3-.9.7-.9Z" />
    </svg>
  ),
};

type Platform = {
  key: string;
  name: string;
  format: string;
  cta: string;
  href: string;
  /** Terminal one-liner that installs the latest release. */
  cmd: string;
  req: string;
  /** Other formats for the same OS. */
  alt?: { label: string; href: string }[];
  featured?: boolean;
};

const PLATFORMS: Platform[] = [
  {
    key: "mac",
    name: "macOS",
    format: "Apple Silicon · .dmg",
    cta: "Download for macOS",
    href: DOWNLOADS.macos,
    cmd: INSTALL.unix,
    req: "macOS 12 Monterey or later · Apple Silicon (M1 and newer)",
    featured: true,
  },
  {
    key: "win",
    name: "Windows",
    format: "64-bit · .exe installer",
    cta: "Download for Windows",
    href: DOWNLOADS.windows,
    cmd: INSTALL.windows,
    req: "Windows 10 / 11 · WebView2 runtime",
    alt: [{ label: ".msi", href: DOWNLOADS.windowsMsi }],
  },
  {
    key: "linux",
    name: "Linux",
    format: "x86_64 · AppImage",
    cta: "Download for Linux",
    href: DOWNLOADS.linuxAppImage,
    cmd: INSTALL.unix,
    req: "glibc 2.31+ · X11 or Wayland · FUSE for AppImage",
    alt: [
      { label: ".deb", href: DOWNLOADS.linuxDeb },
      { label: ".rpm", href: DOWNLOADS.linuxRpm },
    ],
  },
];

export default function DownloadPage() {
  return (
    <>
      <PageHero
        eyebrow={`Download · v${VERSION}`}
        title="Pick your"
        highlight="platform"
        subtitle="A few megabytes, free and open-source on every OS. Grab a build below, or install from your terminal in one line."
      />

      <section className="mx-auto mt-14 max-w-7xl px-6 sm:px-10">
        <div className="grid gap-4 lg:grid-cols-3">
          {PLATFORMS.map((p, i) => (
            <Reveal key={p.key} delay={i * 0.1}>
              <div
                className={`flex h-full flex-col rounded-card p-6 ${
                  p.featured ? "panel glow-ring" : "panel"
                }`}
              >
                <div className="flex items-center justify-between">
                  <span className="text-glow">{ICONS[p.key]}</span>
                  {p.featured && (
                    <span className="font-mono text-[0.62rem] uppercase tracking-[0.2em] text-glow">
                      most popular
                    </span>
                  )}
                </div>
                <h3 className="mt-6 text-3xl font-medium">{p.name}</h3>
                <p className="mt-2 text-sm text-fog">{p.format}</p>

                <div className="mt-7">
                  <Magnetic strength={0.25}>
                    <a
                      href={p.href}
                      className={
                        p.featured
                          ? "btn-glow w-full justify-center"
                          : "btn-ghost w-full justify-center"
                      }
                    >
                      {p.cta}
                    </a>
                  </Magnetic>
                </div>

                {p.alt && (
                  <p className="mt-3 text-center font-mono text-[0.7rem] text-faint">
                    also{" "}
                    {p.alt.map((a, n) => (
                      <span key={a.label}>
                        {n > 0 && " · "}
                        <a href={a.href} className="text-fog hover:text-glow">
                          {a.label}
                        </a>
                      </span>
                    ))}
                  </p>
                )}

                <div className="mt-6 rounded-xl border border-line bg-ink/50 px-4 py-3">
                  <p className="font-mono text-[0.6rem] uppercase tracking-[0.2em] text-faint">
                    or via terminal
                  </p>
                  <code className="mt-1.5 block break-all font-mono text-xs text-glow">
                    {p.cmd}
                  </code>
                </div>

                <p className="mt-5 border-t border-line pt-4 font-mono text-[0.7rem] text-faint">
                  {p.req}
                </p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      {/* Unsigned-build notice. Users hit this dialog on first launch; saying so
          here is the difference between "expected" and "this looks like malware". */}
      <section className="mx-auto mt-10 max-w-7xl px-6 sm:px-10">
        <Reveal>
          <div className="panel rounded-card p-6">
            <h3 className="text-xl font-medium">
              Echo isn&apos;t code-signed yet
            </h3>
            <p className="mt-2 max-w-3xl text-fog">
              Signing certificates cost money this project hasn&apos;t spent, so
              your OS will warn you the first time you open it. That&apos;s
              expected, and here&apos;s exactly what you&apos;ll see:
            </p>
            <ul className="mt-4 space-y-2 text-sm text-fog">
              <li>
                <span className="font-mono text-xs uppercase tracking-[0.18em] text-glow">
                  windows
                </span>{" "}
                — SmartScreen says &ldquo;Windows protected your PC&rdquo;. Choose{" "}
                <em>More info</em> → <em>Run anyway</em>.
              </li>
              <li>
                <span className="font-mono text-xs uppercase tracking-[0.18em] text-glow">
                  macos
                </span>{" "}
                — Gatekeeper offers only <em>Move to Trash</em>. The terminal
                installer clears the quarantine flag for you; installing by hand,
                right-click Echo in Applications → <em>Open</em>.
              </li>
              <li>
                <span className="font-mono text-xs uppercase tracking-[0.18em] text-glow">
                  linux
                </span>{" "}
                — nothing to do.
              </li>
            </ul>
            <p className="mt-4 text-sm text-fog">
              Every build is checksummed. Verify your download against{" "}
              <a
                href={DOWNLOADS.checksums}
                className="text-glow underline underline-offset-4"
              >
                SHA256SUMS.txt
              </a>{" "}
              — the terminal installers do it automatically. Or{" "}
              <a
                href={LINKS.installing}
                target="_blank"
                rel="noreferrer"
                className="text-glow underline underline-offset-4"
              >
                read the install docs
              </a>
              .
            </p>
          </div>
        </Reveal>
      </section>

      {/* build from source */}
      <section className="mx-auto mt-10 max-w-7xl px-6 sm:px-10">
        <Reveal>
          <div className="panel flex flex-col items-start justify-between gap-6 rounded-card p-6 sm:flex-row sm:items-center">
            <div>
              <h3 className="text-xl font-medium">Prefer to build it yourself?</h3>
              <p className="mt-2 max-w-xl text-fog">
                The full source is on GitHub. Clone it, audit it, and compile with
                a single command — that&apos;s the whole point of local-first.
              </p>
            </div>
            <div className="flex shrink-0 gap-3">
              <a
                href={LINKS.releaseNotes}
                target="_blank"
                rel="noreferrer"
                className="btn-ghost"
              >
                Release notes
              </a>
              <a
                href={LINKS.github}
                target="_blank"
                rel="noreferrer"
                className="btn-ghost"
              >
                View source
              </a>
            </div>
          </div>
        </Reveal>
      </section>

      {/* requirements */}
      <section className="mx-auto mt-20 max-w-5xl px-6 sm:px-10">
        <Reveal>
          <h2 className="text-center text-3xl font-semibold sm:text-4xl">
            What you&apos;ll need
          </h2>
        </Reveal>
        <div className="mt-10 grid gap-4 sm:grid-cols-3">
          {[
            { k: "memory", v: "4 GB RAM for tiny models, 8 GB+ for larger ones." },
            { k: "disk", v: "~75 MB app, plus the model you choose (40 MB–3 GB)." },
            { k: "microphone", v: "Any input device your OS recognizes." },
          ].map((r, i) => (
            <Reveal key={r.k} delay={i * 0.08}>
              <div className="panel h-full rounded-card p-6">
                <p className="font-mono text-xs uppercase tracking-[0.2em] text-glow">
                  {r.k}
                </p>
                <p className="mt-3 leading-relaxed text-fog">{r.v}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>
    </>
  );
}
