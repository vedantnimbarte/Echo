import Reveal from "@/components/ui/Reveal";
import TiltCard from "@/components/ui/TiltCard";

/* Labelled by the mechanism behind each one — that's the useful thing to know
   about a tool you're going to leave running all day. */
const FEATURES = [
  {
    label: "openwakeword · opt-in",
    title: "Wake word",
    body: "Arm a phrase and dictate without reaching for the keyboard. Four pretrained phrases ship with Echo; train your own and import the file. Off until you turn it on, and your mic indicator tells the truth while it's armed.",
  },
  {
    label: "foreground app",
    title: "Per-app profiles",
    body: "Override the insert method, the dictionary, and auto-insert for one application at a time. Never auto-type into your password manager without changing how Echo behaves anywhere else.",
  },
  {
    label: "type or paste",
    title: "Two ways in",
    body: "Keystrokes work in every app. Clipboard paste is faster for long transcripts and puts your clipboard back the way it was. Pick per app, or globally.",
  },
  {
    label: "replacement pipeline",
    title: "Custom dictionary",
    body: "Teach Echo the names, jargon, and symbols it keeps getting wrong. Corrections apply before the transcript is shown — and a fix you make by hand can be promoted into the dictionary.",
  },
  {
    label: "sqlite · exportable",
    title: "History",
    body: "Every transcript, grouped by day and searchable. Copy one, re-inject it into the app you're in now, export the lot to JSON, or delete all of it.",
  },
  {
    label: "whisper · autodetect",
    title: "Languages",
    body: "Pin the language you're dictating in, or let Whisper work it out. Pinning it is faster and more accurate when you already know.",
  },
  {
    label: "cmd/ctrl + shift + space",
    title: "Global hotkey",
    body: "Toggle recording from any app, on any screen. Rebind it by pressing the chord you want — if another app already owns it, Echo tells you.",
  },
  {
    label: "echo-sdk · rust",
    title: "Plugins",
    body: "Native plugins can add transcription engines, output targets, audio processing, and dictionaries. Installing one shows you what it asks for and waits for you to agree.",
  },
  {
    label: "signed releases",
    title: "Updates you approve",
    body: "Echo checks GitHub Releases, verifies the signature, and asks before installing. Switch the check off entirely and it stops calling home at all.",
  },
];

export default function FeatureIndex() {
  return (
    <section className="mx-auto mt-28 max-w-7xl px-6 sm:mt-40 sm:px-10">
      <Reveal>
        <div className="flex flex-col justify-between gap-6 md:flex-row md:items-end">
          <div>
            <p className="eyebrow">Everything else</p>
            <h2 className="mt-5 max-w-xl text-5xl sm:text-6xl">
              The rest of the <span className="glow-text">instrument</span>.
            </h2>
          </div>
          <p className="max-w-sm leading-relaxed text-fog">
            Nine things you will actually change in the first week, and can find
            again without hunting.
          </p>
        </div>
      </Reveal>

      <div className="mt-14 grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {FEATURES.map((f, i) => (
          <Reveal key={f.title} delay={(i % 3) * 0.06}>
            <TiltCard className="panel lift rounded-[var(--radius-card)] p-6">
              <p className="datum">{f.label}</p>
              <h3 className="mt-4 text-2xl">{f.title}</h3>
              <p className="mt-3 text-sm leading-relaxed text-fog">{f.body}</p>
            </TiltCard>
          </Reveal>
        ))}
      </div>
    </section>
  );
}
