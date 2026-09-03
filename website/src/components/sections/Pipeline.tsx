import Reveal from "@/components/ui/Reveal";
import PillMock from "@/components/site/PillMock";

/**
 * These are numbered because they genuinely are a sequence — audio enters at
 * the top and leaves as keystrokes at the bottom, and each stage can only run
 * once the one above it has.
 */
const STAGES = [
  {
    no: "01",
    engine: "silero vad",
    title: "The mic stays shut",
    body: "Nothing is captured until you press your shortcut — or, if you armed the wake word, until the voice-activity detector hears speech at all. An idle room costs one VAD pass, not a wake-word inference.",
  },
  {
    no: "02",
    engine: "openwakeword · onnx",
    title: "The phrase lands",
    body: "A 32-bin mel spectrogram, a 96-dimension embedding, then a classifier trained on your phrase. Three small models, downloaded once, running on your own silicon.",
  },
  {
    no: "03",
    engine: "whisper.cpp · local",
    title: "Speech becomes text",
    body: "Whisper transcribes on your CPU by default — offline, no account. Want cloud accuracy instead? Bring an OpenAI, Groq, or Deepgram key; it lives in the OS keychain and never comes back out.",
  },
  {
    no: "04",
    engine: "dictionary",
    title: "Spelled your way",
    body: "Names, jargon, and code symbols are corrected before the transcript ever reaches the page. Fix one by hand in History and you can promote it into the dictionary for good.",
  },
  {
    no: "05",
    engine: "sendinput · cgevent · xdotool",
    title: "It types",
    body: "Keystrokes into whatever app has focus, or a clipboard paste that puts your clipboard back afterwards. Per-app profiles pick which, application by application.",
  },
];

export default function Pipeline() {
  return (
    <section id="pipeline" className="mx-auto mt-28 max-w-7xl px-6 sm:mt-40 sm:px-10">
      <div className="grid gap-14 lg:grid-cols-[0.9fr_1.1fr] lg:gap-20">
        <div className="lg:sticky lg:top-32 lg:self-start">
          <Reveal>
            <p className="eyebrow">The signal path</p>
            <h2 className="mt-5 text-5xl sm:text-6xl">
              Five stages,
              <br />
              one <span className="glow-text">machine</span>.
            </h2>
            <p className="mt-6 max-w-md text-lg leading-relaxed text-fog">
              Every stage between the room and your cursor runs locally unless
              you deliberately point one of them at a cloud provider. Here is the
              whole chain, in order.
            </p>
          </Reveal>

          <Reveal delay={0.1} className="mt-10">
            <PillMock />
            <p className="datum mt-4 max-w-xs leading-relaxed">
              The pill is the whole interface — it sits above your work and
              stays out of the way.
            </p>
          </Reveal>
        </div>

        <ol className="relative">
          {/* the connector the audio travels down */}
          <div
            className="rail absolute bottom-6 left-[27px] top-6 hidden w-px bg-glow/20 sm:block"
            aria-hidden
          />

          {STAGES.map((s, i) => (
            <Reveal key={s.no} delay={i * 0.06}>
              <li className="relative flex gap-6 pb-12 last:pb-0">
                <span className="relative z-10 hidden h-14 w-14 shrink-0 items-center justify-center rounded-full border border-line-2 bg-ink font-mono text-sm text-glow sm:flex">
                  {s.no}
                </span>
                <div className="min-w-0">
                  <div className="flex flex-wrap items-baseline gap-3">
                    <h3 className="text-2xl sm:text-3xl">{s.title}</h3>
                    <span className="datum">{s.engine}</span>
                  </div>
                  <p className="mt-3 leading-relaxed text-fog">{s.body}</p>
                </div>
              </li>
            </Reveal>
          ))}
        </ol>
      </div>
    </section>
  );
}
