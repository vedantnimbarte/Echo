"use client";

import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import Reveal from "@/components/ui/Reveal";
import TiltCard from "@/components/ui/TiltCard";

const SOURCE =
  "we should probably move the sync to thursday because half the team is out on wednesday and the room is booked anyway";

const COMMANDS = [
  {
    say: "echo, fix the punctuation",
    result:
      "We should probably move the sync to Thursday — half the team is out on Wednesday, and the room is booked anyway.",
  },
  {
    say: "echo, make this a one-line message",
    result: "Moving the sync to Thursday: Wednesday is short-staffed and the room's taken.",
  },
  {
    say: "echo, turn this into bullets",
    result: "• Move the sync to Thursday\n• Half the team is out Wednesday\n• The room is already booked",
  },
];

const EASE = [0.16, 1, 0.3, 1] as const;

export default function CommandMode() {
  const [i, setI] = useState(0);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (paused) return;
    const id = setInterval(() => setI((v) => (v + 1) % COMMANDS.length), 5200);
    return () => clearInterval(id);
  }, [paused]);

  return (
    <section className="mx-auto mt-28 max-w-7xl px-6 sm:mt-40 sm:px-10">
      <div className="grid items-center gap-14 lg:grid-cols-[0.85fr_1.15fr] lg:gap-20">
        <Reveal>
          <p className="eyebrow">Command mode</p>
          <h2 className="mt-5 text-5xl sm:text-6xl">
            Say what you
            <br />
            <span className="glow-text">want done</span>.
          </h2>
          <p className="mt-6 max-w-md text-lg leading-relaxed text-fog">
            Start a sentence with your trigger word and Echo stops typing and
            starts working. Select some text, say what should happen to it, and
            the result replaces it in place.
          </p>
          <p className="mt-5 max-w-md leading-relaxed text-fog">
            The model is your choice: Ollama on your own machine by default, or
            OpenAI if you would rather. Off until you switch it on.
          </p>
          <p className="datum mt-7">ollama · openai · off by default</p>
        </Reveal>

        <Reveal delay={0.08}>
          <TiltCard
            className="panel rounded-[var(--radius-card)] p-5 sm:p-7"
            max={4}
          >
            <div
              onMouseEnter={() => setPaused(true)}
              onMouseLeave={() => setPaused(false)}
            >
              <div className="flex items-center justify-between">
                <span className="datum uppercase tracking-[0.24em]">selection</span>
                <span className="datum">notes.md</span>
              </div>
              <p className="mt-3 rounded-xl bg-glow/10 p-3 text-sm leading-relaxed text-fog ring-1 ring-glow/25">
                {SOURCE}
              </p>

              {/* what you say */}
              <div className="mt-5 flex flex-wrap gap-2">
                {COMMANDS.map((c, idx) => (
                  <button
                    key={c.say}
                    onClick={() => setI(idx)}
                    aria-pressed={i === idx}
                    className={`flex items-center gap-2 rounded-full border px-3 py-1.5 font-mono text-[0.7rem] transition-colors ${
                      i === idx
                        ? "border-glow/45 bg-glow/10 text-text"
                        : "border-line-2 text-faint hover:text-fog"
                    }`}
                  >
                    <svg viewBox="0 0 16 16" className="h-3 w-3 fill-current" aria-hidden>
                      <path d="M8 1a2 2 0 0 0-2 2v5a2 2 0 1 0 4 0V3a2 2 0 0 0-2-2Zm-4.5 7a.5.5 0 0 0-1 0 5.5 5.5 0 0 0 5 5.48V15h-2a.5.5 0 0 0 0 1h5a.5.5 0 0 0 0-1h-2v-1.52a5.5 5.5 0 0 0 5-5.48.5.5 0 0 0-1 0 4.5 4.5 0 1 1-9 0Z" />
                    </svg>
                    {c.say}
                  </button>
                ))}
              </div>

              <div className="mt-5 flex items-center gap-3">
                <span className="datum shrink-0">result</span>
                <span className="h-px flex-1 bg-line" />
              </div>

              <div className="mt-3 min-h-[92px]">
                <AnimatePresence mode="wait">
                  <motion.p
                    key={i}
                    initial={{ opacity: 0, y: 8 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -6 }}
                    transition={{ duration: 0.4, ease: EASE }}
                    className="whitespace-pre-line text-base leading-relaxed text-text"
                  >
                    {COMMANDS[i].result}
                  </motion.p>
                </AnimatePresence>
              </div>
            </div>
          </TiltCard>
        </Reveal>
      </div>
    </section>
  );
}
