import Reveal from "@/components/ui/Reveal";

const STEPS = [
  {
    no: "01",
    key: "⌥ Space",
    title: "Summon the pill",
    body: "A global hotkey brings Echo up over any window. No app to switch to, no focus to lose.",
  },
  {
    no: "02",
    key: "speak",
    title: "Just talk",
    body: "The waveform answers your voice while Whisper transcribes it locally — punctuation and all.",
  },
  {
    no: "03",
    key: "↵ typed",
    title: "It lands as text",
    body: "Echo injects the finished words into whatever you were doing. Keep your hands on the keys.",
  },
];

export default function HowItWorks() {
  return (
    <section className="mx-auto mt-24 max-w-7xl px-6 sm:px-10">
      <Reveal className="max-w-2xl">
        <p className="eyebrow">Three seconds, start to finish</p>
        <h2 className="mt-4 text-5xl font-semibold sm:text-6xl">
          The whole loop is
          <span className="signal-gradient"> one breath</span>.
        </h2>
      </Reveal>

      <div className="relative mt-14">
        {/* glowing connector */}
        <div
          aria-hidden
          className="absolute left-0 right-0 top-7 hidden h-px bg-gradient-to-r from-transparent via-glow/50 to-transparent lg:block"
        />
        <div className="grid gap-10 lg:grid-cols-3">
          {STEPS.map((s, i) => (
            <Reveal key={s.no} delay={i * 0.06}>
              <div className="relative">
                <div className="flex items-center gap-4">
                  <span className="glass glow-ring flex h-14 w-14 items-center justify-center rounded-full font-mono text-sm text-glow">
                    {s.no}
                  </span>
                  <span className="font-mono text-xs uppercase tracking-[0.22em] text-faint">
                    {s.key}
                  </span>
                </div>
                <h3 className="mt-6 text-3xl font-medium">{s.title}</h3>
                <p className="mt-3 max-w-sm text-lg leading-relaxed text-fog">
                  {s.body}
                </p>
              </div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
