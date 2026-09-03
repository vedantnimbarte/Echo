import type { Metadata } from "next";
import PageHero from "@/components/site/PageHero";
import Reveal from "@/components/ui/Reveal";
import FinalCTA from "@/components/sections/FinalCTA";
import { LINKS } from "@/lib/links";

export const metadata: Metadata = {
  title: "Privacy — Echo",
  description:
    "What Echo sends, what it stores, and what it can't prove. The request log, local-only telemetry, keychain key storage, and the limits of each claim.",
};

const LEAVES = [
  {
    when: "You add a cloud provider",
    what: "Your audio goes to OpenAI, Groq, or Deepgram — the one you chose, with your key.",
    tone: "sends",
  },
  {
    when: "You enable a model download",
    what: "Echo fetches the Whisper model or wake-word models once, from GitHub, then stops.",
    tone: "sends",
  },
  {
    when: "The update check is on",
    what: "Echo asks GitHub Releases whether a newer version exists. Switch it off and it stops asking.",
    tone: "sends",
  },
  {
    when: "You dictate locally",
    what: "Nothing. Audio goes to a model on your own CPU and the transcript goes to your cursor.",
    tone: "quiet",
  },
  {
    when: "Telemetry is on",
    what: "Nothing leaves. Counts go into SQLite on this machine, where you can read and delete them.",
    tone: "quiet",
  },
];

const STORES = [
  {
    title: "Transcripts",
    body: "History lives in a local SQLite database. Search it, export it to JSON, or clear it. Turning History off stops Echo writing to it at all.",
  },
  {
    title: "API keys",
    body: "Stored in the OS keychain — Credential Manager on Windows, Keychain on macOS, Secret Service on Linux — and never returned to the interface that saved them.",
  },
  {
    title: "Telemetry",
    body: "Opt-in. Event counts and coarse metadata like word count. Never audio, transcript text, file paths, or window titles.",
  },
  {
    title: "Models",
    body: "Whisper and wake-word models are downloaded once into Echo's app data directory and used offline from then on.",
  },
];

export default function PrivacyPage() {
  return (
    <>
      <PageHero
        eyebrow="Privacy"
        title="What leaves, and what"
        highlight="doesn't"
        subtitle="Dictation means handing a program your microphone. Here is exactly what Echo does with it — and where the honest limits of that claim are."
      />

      <section className="mx-auto mt-20 max-w-5xl px-6 sm:px-10">
        <Reveal>
          <p className="eyebrow">Outbound</p>
          <h2 className="mt-4 text-4xl sm:text-5xl">
            Five moments Echo <span className="glow-text">could</span> speak.
          </h2>
        </Reveal>

        <div className="mt-10 divide-y divide-line border-y border-line">
          {LEAVES.map((l, i) => (
            <Reveal key={l.when} delay={i * 0.05}>
              <div className="grid gap-2 py-6 sm:grid-cols-[1fr_1.4fr] sm:gap-8">
                <div className="flex items-baseline gap-3">
                  <span
                    className={`mt-2 h-1.5 w-1.5 shrink-0 rounded-full ${
                      l.tone === "sends" ? "bg-ember" : "bg-glow"
                    }`}
                    aria-hidden
                  />
                  <h3 className="text-lg font-semibold">{l.when}</h3>
                </div>
                <p className="leading-relaxed text-fog">{l.what}</p>
              </div>
            </Reveal>
          ))}
        </div>

        <Reveal delay={0.2}>
          <p className="mt-6 text-sm text-faint">
            Settings → Privacy shows whether your current setup can reach the
            network at all, plus the host, reason, and time of every request Echo
            has made.
          </p>
        </Reveal>
      </section>

      <section className="mx-auto mt-24 max-w-5xl px-6 sm:mt-32 sm:px-10">
        <Reveal>
          <p className="eyebrow">At rest</p>
          <h2 className="mt-4 text-4xl sm:text-5xl">
            What Echo <span className="glow-text">keeps</span>.
          </h2>
        </Reveal>

        <div className="mt-10 grid gap-4 sm:grid-cols-2">
          {STORES.map((s, i) => (
            <Reveal key={s.title} delay={(i % 2) * 0.06}>
              <div className="panel h-full rounded-[var(--radius-card)] p-6">
                <h3 className="text-xl font-semibold">{s.title}</h3>
                <p className="mt-3 text-sm leading-relaxed text-fog">{s.body}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      <section className="mx-auto mt-24 max-w-3xl px-6 sm:mt-32 sm:px-10">
        <Reveal>
          <p className="eyebrow">Limits</p>
          <h2 className="mt-4 text-4xl sm:text-5xl">
            Three things this <span className="glow-text">can&rsquo;t</span> prove.
          </h2>

          <div className="mt-10 space-y-8 leading-relaxed text-fog">
            <p>
              <strong className="font-semibold text-text">
                The request log only sees Echo.
              </strong>{" "}
              It records requests Echo itself made. No process can observe its
              own operating system&rsquo;s traffic, so this is evidence, not a
              packet capture.
            </p>
            <p>
              <strong className="font-semibold text-text">
                Plugins are outside it.
              </strong>{" "}
              A native plugin runs as real code in the process and can make
              requests that never pass through the code the log instruments.
              That is why installing one shows you what it asks for and waits.{" "}
              <a
                href={LINKS.plugins}
                target="_blank"
                rel="noreferrer"
                className="text-fog underline underline-offset-4 hover:text-glow"
              >
                The plugin contract is public
              </a>
              .
            </p>
            <p>
              <strong className="font-semibold text-text">
                An armed mic is an open mic.
              </strong>{" "}
              With the wake word on, your OS indicator stays lit — because it
              should. Audio is processed and discarded locally, but the
              microphone is genuinely open, and no wording changes that.
            </p>
          </div>

          <p className="mt-10 border-l-2 border-line-2 pl-5 text-sm leading-relaxed text-faint">
            The most complete answer to all three is the one open source gives
            you:{" "}
            <a
              href={LINKS.github}
              target="_blank"
              rel="noreferrer"
              className="text-fog underline underline-offset-4 hover:text-glow"
            >
              read the code
            </a>
            , or build it yourself and run what you compiled.
          </p>
        </Reveal>
      </section>

      <FinalCTA />
    </>
  );
}
