"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import Logo from "./Logo";
import Magnetic from "@/components/ui/Magnetic";
import { LINKS as EXT } from "@/lib/links";

const LINKS = [
  { href: "/features", label: "Features", idx: "01" },
  { href: "/pricing", label: "Pricing", idx: "02" },
  { href: "/download", label: "Download", idx: "03" },
];

export default function Nav() {
  const [scrolled, setScrolled] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const onScroll = () => setScrolled(window.scrollY > 24);
    onScroll();
    window.addEventListener("scroll", onScroll, { passive: true });
    return () => window.removeEventListener("scroll", onScroll);
  }, []);

  return (
    <header className="fixed inset-x-0 top-0 z-50">
      <div
        className={`mx-auto mt-3 flex max-w-7xl items-center justify-between gap-6 rounded-full px-4 py-2.5 transition-all duration-300 sm:px-5 ${
          scrolled
            ? "glass mx-3 shadow-[0_8px_40px_-20px_rgba(0,0,0,0.9)] sm:mx-6"
            : "border border-transparent"
        }`}
      >
        <Logo />

        <nav className="hidden items-center gap-1 md:flex">
          {LINKS.map((l) => (
            <Link
              key={l.href}
              href={l.href}
              className="group relative flex items-center gap-2 rounded-full px-4 py-2 text-sm text-fog transition-colors hover:text-text"
            >
              <span className="font-mono text-[0.62rem] text-faint transition-colors group-hover:text-glow">
                {l.idx}
              </span>
              {l.label}
            </Link>
          ))}
        </nav>

        <div className="flex items-center gap-2">
          <a
            href={EXT.github}
            target="_blank"
            rel="noreferrer"
            className="hidden items-center gap-2 rounded-full px-3 py-2 text-sm text-fog transition-colors hover:text-text sm:inline-flex"
          >
            <svg viewBox="0 0 24 24" className="h-4 w-4 fill-current" aria-hidden>
              <path d="M12 .5C5.7.5.5 5.7.5 12c0 5.1 3.3 9.4 7.9 10.9.6.1.8-.2.8-.6v-2c-3.2.7-3.9-1.4-3.9-1.4-.5-1.3-1.3-1.7-1.3-1.7-1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1 1.8 2.7 1.3 3.4 1 .1-.8.4-1.3.7-1.6-2.6-.3-5.3-1.3-5.3-5.7 0-1.3.5-2.3 1.2-3.1-.1-.3-.5-1.5.1-3.1 0 0 1-.3 3.3 1.2a11.5 11.5 0 0 1 6 0C18 4.6 19 4.9 19 4.9c.6 1.6.2 2.8.1 3.1.8.8 1.2 1.8 1.2 3.1 0 4.4-2.7 5.4-5.3 5.7.4.4.8 1.1.8 2.2v3.3c0 .4.2.7.8.6 4.6-1.5 7.9-5.8 7.9-10.9C23.5 5.7 18.3.5 12 .5Z" />
            </svg>
            <span className="sr-only">GitHub</span>
          </a>

          <Magnetic className="hidden sm:inline-block">
            <Link href="/download" className="btn-glow text-sm">
              Download
            </Link>
          </Magnetic>

          <button
            onClick={() => setOpen((v) => !v)}
            className="btn-ghost px-3 py-2 md:hidden"
            aria-label="Toggle menu"
          >
            <span className="block h-[2px] w-5 bg-current" />
          </button>
        </div>
      </div>

      <AnimatePresence>
        {open && (
          <motion.nav
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="glass mx-3 mt-2 flex flex-col gap-1 rounded-3xl p-3 md:hidden"
          >
            {LINKS.map((l) => (
              <Link
                key={l.href}
                href={l.href}
                onClick={() => setOpen(false)}
                className="flex items-center justify-between rounded-2xl px-4 py-3 text-text hover:bg-ink-2"
              >
                {l.label}
                <span className="font-mono text-xs text-glow">{l.idx}</span>
              </Link>
            ))}
          </motion.nav>
        )}
      </AnimatePresence>
    </header>
  );
}
