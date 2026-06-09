"use client";

import { motion } from "motion/react";

const EASE = [0.22, 1, 0.36, 1] as const;

/** Scroll-triggered reveal — a quick, subtle fade-up. Kept intentionally
 *  understated so scrolling feels calm and modern, not animated. */
export default function Reveal({
  children,
  delay = 0,
  y = 10,
  className = "",
  once = true,
}: {
  children: React.ReactNode;
  delay?: number;
  y?: number;
  className?: string;
  once?: boolean;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once, margin: "0px 0px -10% 0px" }}
      transition={{ duration: 0.4, delay, ease: EASE }}
      className={className}
    >
      {children}
    </motion.div>
  );
}
