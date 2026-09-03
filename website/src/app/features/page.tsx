import type { Metadata } from "next";
import PageHero from "@/components/site/PageHero";
import Reveal from "@/components/ui/Reveal";
import TiltCard from "@/components/ui/TiltCard";
import FinalCTA from "@/components/sections/FinalCTA";
import { LINKS } from "@/lib/links";

export const metadata: Metadata = {
  title: "Features — Echo",
  description:
    "Wake word, local Whisper transcription, command mode, per-app profiles, dictionaries, history, and plugins — what each one does, and what it costs you.",
};

type Group = {
  eyebrow: string;
  title: string;
  highlight: string;
  intro: string;
  items: { label: string; title: string; body: string }[];
};

const GROUPS: Group[] = [
  {
    eyebrow: "Capture",
    title: "Getting Echo",
    highlight: "listening",
    intro:
      "Three ways to start a dictation, from most deliberate to entirely hands-free. Nothing is captured until one of them fires.",
    items: [
      {
        label: "cmd/ctrl + shift + space",
        title: "The hotkey",
        body: "The default. Press once to start, once to stop, from any app on any screen. Rebind it by pressing the chord you want — if another app already owns it, Echo says so instead of failing silently.",
      },
      {
        label: "silero vad",
        title: "Auto mode",
        body: "Arm it once and keep talking. Voice-activity detection finds the edges of each utterance and transcribes segment by segment, so a long thought doesn't need a button held down.",
      },
      {
        label: "openwakeword · opt-in",
        title: "Wake word",
        body: "Say a phrase and start dictating with your hands still on the keyboard. Ships with Hey Jarvis, Alexa, Hey Mycroft, and Hey Rhasspy; a custom phrase is an .onnx file you train and import.",
      },
      {
        label: "the honest part",
        title: "What armed listening costs",
        body: "Your OS microphone indicator stays lit the whole time — correctly, because the mic really is open. Sensitivity trades false accepts against false rejects, and no setting removes both.",
      },
    ],
  },
  {
    eyebrow: "Transcription",
    title: "Turning sound into",
    highlight: "words",
    intro:
      "Local by default. Cloud only if you go and ask for it, with your own key.",
    items: [
      {
        label: "whisper.cpp",
        title: "Local Whisper",
        body: "Runs on your CPU with no account and no connection. Pick a model size that fits your machine — tiny for instant, larger for accuracy — and Echo downloads it once.",
      },
      {
        label: "openai · groq · deepgram",
        title: "Bring your own key",
        body: "Prefer a hosted model? Add a key and Echo routes to it. The key goes into the OS keychain and is never handed back to the interface that stored it. Echo adds no markup and takes no cut.",
      },
      {
        label: "autodetect or pinned",
        title: "Languages",
        body: "Let Whisper work out what you're speaking, or pin the language. Pinning is faster and more accurate when you already know, which is most of the time.",
      },
      {
        label: "replacement pipeline",
        title: "Dictionary",
        body: "Names, jargon, and code symbols corrected before the transcript is ever shown. Fix one by hand in History and you can promote that correction into the dictionary permanently.",
      },
    ],
  },
  {
    eyebrow: "Output",
    title: "Where the text",
    highlight: "lands",
    intro:
      "Echo speaks your OS's keyboard language, so the destination is simply whatever has focus.",
    items: [
      {
        label: "sendinput · cgevent · xdotool",
        title: "Type keystrokes",
        body: "The universal method. Works in editors, terminals, browser fields, and anything else that accepts a keyboard.",
      },
      {
        label: "clipboard + restore",
        title: "Paste instead",
        body: "Faster and more reliable for long transcripts: Echo puts the text on the clipboard, sends the paste shortcut, and puts your clipboard back. Some terminals use a different paste chord — worth checking.",
      },
      {
        label: "foreground app",
        title: "Per-app profiles",
        body: "Override the insert method, the dictionary, and auto-insert for one app at a time; every field can stay on Global. Where the platform won't say which window is focused — Wayland, or macOS without Automation permission — profiles quietly don't apply.",
      },
      {
        label: "sqlite · exportable",
        title: "History",
        body: "Every transcript, grouped by day, searchable. Copy one, re-inject it where your cursor is now, export the lot to JSON, or delete all of it.",
      },
    ],
  },
  {
    eyebrow: "Beyond dictation",
    title: "Instructions, not",
    highlight: "transcripts",
    intro:
      "Two features that make Echo do something with your words instead of just writing them down. Both off by default.",
    items: [
      {
        label: "ollama · openai",
        title: "Command mode",
        body: "Open a sentence with your trigger word and Echo sends it to an LLM instead of typing it. With text selected, the instruction is applied to that selection and the result replaces it.",
      },
      {
        label: "echo-sdk · rust",
        title: "Plugins",
        body: "Native plugins can add transcription engines, output targets, audio processing, and dictionaries. Installing one shows you what it asks for first and waits for you to agree.",
      },
      {
        label: "signed releases",
        title: "Updates",
        body: "Echo checks GitHub Releases, verifies the signature, and asks before installing. Switch the check off and it stops making that request at all.",
      },
      {
        label: "local-only telemetry",
        title: "Usage counts",
        body: "Opt-in, stored in SQLite here, never transmitted. Counts and coarse metadata only — no audio, no transcript text, no window titles. Viewable and deletable.",
      },
    ],
  },
];

const PLATFORMS = [
  {
    os: "Windows",
    setup: "Works out of the box",
    detail: "Needs the WebView2 runtime — preinstalled on Windows 11.",
  },
  {
    os: "macOS",
    setup: "Apple Silicon only",
    detail:
      "Grant Microphone and Accessibility on first run. There's no Intel build: ONNX Runtime publishes no Intel-macOS binaries.",
  },
  {
    os: "Linux",
    setup: "X11 or Wayland",
    detail:
      "Text injection needs xdotool (X11) or ydotool with its daemon (Wayland). The AppImage needs FUSE; a .deb is attached to every release.",
  },
];

export default function FeaturesPage() {
  return (
    <>
      <PageHero
        eyebrow="Features"
        title="Every switch, and what it"
        highlight="costs"
        subtitle="Echo has a lot of settings because dictation is personal. Here is what each one does — including the ones with a real trade-off attached."
      />

      {GROUPS.map((g, gi) => (
        <section
          key={g.title}
          className={`mx-auto max-w-7xl px-6 sm:px-10 ${gi === 0 ? "mt-20" : "mt-24 sm:mt-32"}`}
        >
          <Reveal>
            <div className="flex flex-col justify-between gap-6 border-b border-line pb-8 md:flex-row md:items-end">
              <div>
                <p className="eyebrow">{g.eyebrow}</p>
                <h2 className="mt-4 text-4xl sm:text-5xl">
                  {g.title} <span className="glow-text">{g.highlight}</span>.
                </h2>
              </div>
              <p className="max-w-sm leading-relaxed text-fog">{g.intro}</p>
            </div>
          </Reveal>

          <div className="mt-8 grid gap-4 sm:grid-cols-2">
            {g.items.map((item, i) => (
              <Reveal key={item.title} delay={(i % 2) * 0.06}>
                <TiltCard className="panel lift rounded-[var(--radius-card)] p-6">
                  <p className="datum">{item.label}</p>
                  <h3 className="mt-4 text-2xl">{item.title}</h3>
                  <p className="mt-3 text-sm leading-relaxed text-fog">
                    {item.body}
                  </p>
                </TiltCard>
              </Reveal>
            ))}
          </div>
        </section>
      ))}

      <section className="mx-auto mt-24 max-w-7xl px-6 sm:mt-32 sm:px-10">
        <Reveal>
          <p className="eyebrow">Platforms</p>
          <h2 className="mt-4 text-4xl sm:text-5xl">
            What each OS <span className="glow-text">asks of you</span>.
          </h2>
        </Reveal>

        <div className="mt-8 grid gap-4 lg:grid-cols-3">
          {PLATFORMS.map((p, i) => (
            <Reveal key={p.os} delay={i * 0.06}>
              <div className="panel h-full rounded-[var(--radius-card)] p-6">
                <h3 className="text-2xl">{p.os}</h3>
                <p className="datum mt-2">{p.setup}</p>
                <p className="mt-4 text-sm leading-relaxed text-fog">{p.detail}</p>
              </div>
            </Reveal>
          ))}
        </div>

        <Reveal delay={0.12}>
          <p className="mt-6 text-sm text-faint">
            Wake word and command mode have their own guide —{" "}
            <a
              href={LINKS.wakeWord}
              target="_blank"
              rel="noreferrer"
              className="text-fog underline underline-offset-4 hover:text-glow"
            >
              including how to train &ldquo;Hey Echo&rdquo; yourself
            </a>
            .
          </p>
        </Reveal>
      </section>

      <FinalCTA />
    </>
  );
}
