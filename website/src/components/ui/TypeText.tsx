"use client";

import { useEffect, useState } from "react";

/** Types through a rotating list of phrases with a blinking caret. */
export default function TypeText({
  phrases,
  className = "",
  typeSpeed = 42,
  holdMs = 1600,
}: {
  phrases: string[];
  className?: string;
  typeSpeed?: number;
  holdMs?: number;
}) {
  const [index, setIndex] = useState(0);
  const [text, setText] = useState("");
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    const current = phrases[index % phrases.length];
    const atEnd = !deleting && text === current;
    const atStart = deleting && text === "";
    const delay = atEnd ? holdMs : atStart ? 350 : deleting ? typeSpeed / 1.8 : typeSpeed;

    const timeout = setTimeout(() => {
      if (atEnd) {
        setDeleting(true);
      } else if (atStart) {
        setDeleting(false);
        setIndex((i) => i + 1);
      } else {
        setText((t) =>
          deleting ? current.slice(0, t.length - 1) : current.slice(0, t.length + 1)
        );
      }
    }, delay);

    return () => clearTimeout(timeout);
  }, [text, deleting, index, phrases, typeSpeed, holdMs]);

  return (
    <span className={className}>
      {text}
      <span className="anim-blink ml-0.5 inline-block h-[1em] w-[2px] translate-y-[0.12em] bg-glow align-middle" />
    </span>
  );
}
