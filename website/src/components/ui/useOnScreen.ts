"use client";

import { useEffect, useRef, useState } from "react";

/** True while the element is on screen — so animation loops can stop when it isn't. */
export default function useOnScreen<T extends HTMLElement>() {
  const ref = useRef<T>(null);
  const [onScreen, setOnScreen] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => setOnScreen(e.isIntersecting));
    io.observe(el);
    return () => io.disconnect();
  }, []);

  return [ref, onScreen] as const;
}
