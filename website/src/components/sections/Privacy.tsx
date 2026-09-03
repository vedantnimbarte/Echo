import Reveal from "@/components/ui/Reveal";
import TiltCard from "@/components/ui/TiltCard";

const REQUESTS = [
  { host: "api.github.com", why: "checked for an update", when: "2 days ago" },
  {
    host: "objects.githubusercontent.com",
    why: "downloaded whisper base.en",
    when: "9 days ago",
  },
  {
    host: "github.com",
    why: "downloaded wake-word models",
    when: "9 days ago",
  },
];

const FACTS = [
  {
    title: "Telemetry never leaves",
    body: "Opt-in, stored in SQLite on your machine, and viewable as counts you can delete. No audio, no transcript text, no window titles.",
  },
  {
    title: "Keys live in the keychain",
    body: "If you add a cloud provider, the key goes to the OS keychain and is never handed back to the interface that stored it.",
  },
  {
    title: "Offline is a real mode",
    body: "With local Whisper and the update check off, Echo makes no requests at all — and the log below is how you confirm it.",
  },
];

export default function Privacy() {
  return (
    <section className="mx-auto mt-28 max-w-7xl px-6 sm:mt-40 sm:px-10">
      <div className="grid items-start gap-14 lg:grid-cols-[1.05fr_0.95fr] lg:gap-20">
        <Reveal>
          <TiltCard className="panel rounded-[var(--radius-card)] p-5 sm:p-7" max={4}>
            <div className="flex items-center justify-between gap-4">
              <span className="datum uppercase tracking-[0.24em]">
                requests echo made
              </span>
              <span className="flex items-center gap-2 font-mono text-[0.7rem] text-glow">
                <span className="h-1.5 w-1.5 rounded-full bg-glow" />
                offline-capable
              </span>
            </div>

            <div className="mt-5 divide-y divide-line">
              {REQUESTS.map((r) => (
                <div
                  key={r.host + r.when}
                  className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 py-3"
                >
                  <span className="font-mono text-sm text-text">{r.host}</span>
                  <span className="datum">{r.when}</span>
                  <span className="w-full text-sm text-fog">{r.why}</span>
                </div>
              ))}
            </div>

            <p className="datum mt-5 border-t border-line pt-4">
              nothing since — transcription is running locally
            </p>
          </TiltCard>
        </Reveal>

        <div>
          <Reveal>
            <p className="eyebrow">Privacy</p>
            <h2 className="mt-5 text-5xl sm:text-6xl">
              Proof, not a <span className="glow-text">promise</span>.
            </h2>
            <p className="mt-6 text-lg leading-relaxed text-fog">
              Echo logs every outbound request it makes — the host, the reason,
              and when. Local transcription makes none, so you can watch the log
              stay empty instead of taking our word for it.
            </p>
          </Reveal>

          <div className="mt-10 space-y-7">
            {FACTS.map((f, i) => (
              <Reveal key={f.title} delay={0.06 * (i + 1)}>
                <h3 className="text-lg font-semibold">{f.title}</h3>
                <p className="mt-2 leading-relaxed text-fog">{f.body}</p>
              </Reveal>
            ))}
          </div>

          <Reveal delay={0.24}>
            <p className="mt-10 border-l-2 border-line-2 pl-5 text-sm leading-relaxed text-faint">
              Read that claim precisely. The log records requests{" "}
              <em className="not-italic text-fog">Echo itself</em> made. It is
              not proof that nothing else left your machine — no process can
              observe its own OS&rsquo;s traffic, and a native plugin can make
              requests that never pass through the code this instruments.
            </p>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
