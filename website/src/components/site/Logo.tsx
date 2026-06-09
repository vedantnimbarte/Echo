import Link from "next/link";

/**
 * Echo mark — a stylized recreation of the source logo:
 * a sharp spoken spike on the left that decays and collapses
 * into flowing, parallel horizontal lines (the "typed" output).
 * Pure stroke + currentColor so it themes anywhere.
 */
export function EchoMark({ className = "" }: { className?: string }) {
  return (
    <svg
      viewBox="0 0 128 40"
      fill="none"
      className={className}
      stroke="currentColor"
      strokeWidth={2.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {/* main signal: ripple -> sharp spike -> decaying wave -> flat line */}
      <path d="M2 21 C4 21 5 19.5 7 19.5 C9 19.5 9.5 22 11 22 L13 20.5 L16.5 5 L19 35 L21.5 11 L24 27 C26 19 27.5 23.5 30 20.5 C33 17.5 35 23 38 20.5 C46 16 58 24 70 20.5 C84 17 100 22 126 20.5" />
      {/* upper fan line */}
      <path d="M42 15 C58 13.5 80 16 98 15" opacity="0.75" />
      {/* lower fan line */}
      <path d="M40 25.5 C56 24 78 27.5 104 25.5" opacity="0.75" />
      {/* trailing dashes (output tails) */}
      <path d="M110 15 H120" opacity="0.55" />
      <path d="M112 25.5 H118" opacity="0.55" />
    </svg>
  );
}

export default function Logo({
  wordmark = true,
  className = "",
}: {
  wordmark?: boolean;
  className?: string;
}) {
  return (
    <Link
      href="/"
      className={`group inline-flex items-center gap-2.5 ${className}`}
      aria-label="Echo home"
    >
      <span className="text-glow transition-all duration-500 group-hover:[filter:drop-shadow(0_0_10px_var(--color-glow))]">
        <EchoMark className="h-6 w-[5.2rem]" />
      </span>
      {wordmark && (
        <span className="font-display text-[1.35rem] font-semibold tracking-tight text-text">
          Echo
        </span>
      )}
    </Link>
  );
}
