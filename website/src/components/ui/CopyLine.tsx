"use client";

import { useEffect, useState } from "react";

/** A terminal one-liner you can take with one click. */
export default function CopyLine({
  label,
  command,
}: {
  label: string;
  command: string;
}) {
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!copied) return;
    const id = setTimeout(() => setCopied(false), 1800);
    return () => clearTimeout(id);
  }, [copied]);

  return (
    <div className="glass min-w-0 overflow-hidden rounded-2xl p-4">
      <p className="datum">{label}</p>
      <div className="mt-3 flex items-center gap-3">
        <code className="command min-w-0 flex-1 overflow-x-auto whitespace-nowrap text-xs text-text sm:text-sm">
          {command}
        </code>
        <button
          onClick={() => {
            navigator.clipboard.writeText(command).then(() => setCopied(true));
          }}
          className="shrink-0 rounded-full border border-line-2 px-3 py-1.5 text-[0.7rem] text-fog transition-colors hover:border-line-2 hover:text-text"
        >
          {copied ? "copied" : "copy"}
        </button>
      </div>
    </div>
  );
}
