import { EchoMark } from "@/components/site/Logo";

const CLAIMS = [
  "100% on-device",
  "0 bytes uploaded",
  "90+ languages",
  "open-source",
  "works in any app",
  "global hotkey",
  "no account needed",
  "Whisper + Silero",
  "custom dictionary",
  "plugin-ready",
];

export default function Marquee() {
  return (
    <section className="marquee-pause relative mt-20 overflow-hidden border-y border-line py-5">
      <div className="pointer-events-none absolute inset-y-0 left-0 z-10 w-32 bg-gradient-to-r from-ink to-transparent" />
      <div className="pointer-events-none absolute inset-y-0 right-0 z-10 w-32 bg-gradient-to-l from-ink to-transparent" />
      <div className="marquee-track flex w-max items-center gap-10">
        {[0, 1].map((dup) => (
          <div key={dup} className="flex items-center gap-10" aria-hidden={dup === 1}>
            {CLAIMS.map((c) => (
              <div key={c} className="flex items-center gap-10 whitespace-nowrap">
                <span className="font-mono text-sm uppercase tracking-[0.18em] text-fog">
                  {c}
                </span>
                <EchoMark className="h-3.5 w-12 text-glow/40" />
              </div>
            ))}
          </div>
        ))}
      </div>
    </section>
  );
}
