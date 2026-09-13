//! The About page: what this is, which version of it you have, and how to say
//! it broke.
//!
//! Reporting a bug is the one thing here with a decision in it. Echo does not
//! file the issue: it opens GitHub's own form with the fields already typed in,
//! and the user submits it there, signed in as themselves. That means no token
//! to create, no credential for Echo to hold, and nothing published that the
//! user has not read on the page it is being published to.
//!
//! The diagnostics block is editable *before* it opens for the same reason the
//! egress log exists — this app tells you what leaves it, so a textarea the
//! user can gut is the honest shape for "we would like to attach this".
//!
//! Design notes, for whoever changes this next:
//!
//! - The mark is the page's one graphic and its only loud moment. It is drawn
//!   here at 72px, nearly three times the sidebar's 26px, because everywhere
//!   else it is an icon and here it is the subject. At 48px it read as a stray
//!   icon rather than a decision. Nothing else on the page raises its voice.
//! - No cards. `styles.css` reserves glass for things you act on, and a licence
//!   is not one; the only raised surface here is the diagnostics box, which is
//!   an input. Structure comes from the same hairlines every other page uses.
//! - The version appears once. It used to sit in the identity line *and* beside
//!   the update button, which is two answers to one question.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Check as CheckIcon, AlertTriangle } from "lucide-react";

import { commands } from "../../ipc/commands";
import { checkForUpdate, currentVersion, CHECK_ON_START, type UpdateOutcome } from "../../update";
import { LINKS, LICENSE, issueUrl, open, type IssueKind } from "../../support";
import { Group, Check } from "../common/Page";

/** One line of muted text. Failures and dead ends both land here. */
function Note({ warn = false, children }: { warn?: boolean; children: React.ReactNode }) {
  return (
    <span
      className={
        "flex items-start gap-1.5 text-[13.5px] leading-snug " +
        (warn ? "font-medium text-[var(--ink)]" : "text-[var(--ink-muted)]")
      }
    >
      {warn && <AlertTriangle className="mt-[3px] h-3 w-3 shrink-0" />}
      {children}
    </span>
  );
}

/** What a "Check now" turned out to be, said in one line under the button. */
function UpdateResult({ outcome }: { outcome: UpdateOutcome }) {
  switch (outcome.kind) {
    case "current":
      return (
        <span className="flex items-center gap-1.5 text-[13.5px] font-medium text-[var(--ink)]">
          <CheckIcon className="h-3.5 w-3.5" />
          You are on the latest version.
        </span>
      );
    case "offline":
      return <Note warn>Couldn't reach the release feed. Check your connection.</Note>;
    case "failed":
      return <Note warn>{outcome.message}</Note>;
    // `declined` means a dialog already said everything there was to say.
    case "declined":
      return null;
  }
}

/** The shared look of every link on this page, external or not. */
const LINK_CLASS =
  "text-[13.5px] text-[var(--ink-muted)] underline decoration-[var(--hairline-strong)] underline-offset-[5px] transition-colors hover:text-[var(--ink)] hover:decoration-[var(--ink)]";

/**
 * An external link. A button, not an anchor: these leave the webview through
 * the opener plugin, and an `href` would navigate the app itself.
 */
function Link({ href, children }: { href: string; children: React.ReactNode }) {
  return (
    <button
      onClick={() => void open(href)}
      className={LINK_CLASS}
    >
      {children}
    </button>
  );
}

/**
 * The Echo mark.
 *
 * Speech energy: quiet, a spike, then a decay back to the line — the app's
 * whole job in one stroke. The sidebar draws it at 26px, where it reads as a
 * glyph; at this size it reads as the waveform it is, which is the reason it
 * is here and not there.
 */
function Mark() {
  return (
    <svg
      viewBox="0 0 40 40"
      className="h-[72px] w-[72px] text-[var(--ink)]"
      fill="none"
      stroke="currentColor"
      // Lighter than the sidebar's 2.6 because the figure is nearly three
      // times the size: constant weight would thicken into a blunt scrawl.
      strokeWidth="1.9"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M2 21 C4 21 5 19.5 7 19.5 C9 19.5 9.5 22 11 22 L13 20.5 L16.5 5 L19 35 L21.5 11 L24 27 C26 19 27.5 23.5 30 20.5 C33 17.5 35 23 38 20.5" />
    </svg>
  );
}

/**
 * @param label Prefixes each group with the page it lives on while a search is
 *   running, the way every other group on this window is titled.
 */
export function About({ label }: { label: (title: string) => string }) {
  const qc = useQueryClient();

  const { data: version = "" } = useQuery({
    queryKey: ["app-version"],
    queryFn: currentVersion,
  });
  const { data: gathered } = useQuery({
    queryKey: ["diagnostics"],
    queryFn: commands.diagnostics,
  });

  const { data: checkOnStart } = useQuery({
    queryKey: ["setting", CHECK_ON_START],
    queryFn: () => commands.getSetting(CHECK_ON_START),
  });
  const setCheckOnStart = useMutation({
    mutationFn: (v: string) => commands.setSetting(CHECK_ON_START, v),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["setting", CHECK_ON_START] }),
  });
  const checkUpdate = useMutation({
    // Silent: this page has somewhere to put the answer, so the uneventful ones
    // are a line of text here rather than a dialog to dismiss.
    mutationFn: () => checkForUpdate({ silent: true }),
  });

  // Seeded from the query, then owned by the textarea: once the user has edited
  // it, a refetch must not overwrite what they wrote.
  const [edited, setEdited] = useState<string | null>(null);
  const diagnostics = edited ?? gathered ?? "";
  const [copied, setCopied] = useState(false);

  async function copy() {
    await navigator.clipboard.writeText(diagnostics);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  }

  return (
    <>
      {/* The opening block. No group title: the page is called About and this
          is what it is about. It gets more room beneath it than the groups give
          each other, and the hairline below starts the regular rhythm. */}
      <section className="pb-10">
        <Mark />
        {/* 28px under the mark, 10px under the name: the block reads as one
            object with the figure over it, rather than three stacked rows. */}
        <h3 className="mt-7 text-[19px] font-medium tracking-tight text-[var(--ink)]">
          Echo <span className="tabular text-[var(--ink-muted)]">{version}</span>
        </h3>
        {/* One sentence. It used to also say MIT and name the repo, which is
            what the two links directly beneath it are for. */}
        <p className="mt-2.5 max-w-[46ch] text-[14.5px] leading-relaxed text-[var(--ink-muted)]">
          Speech in, typed text out, on your own machine.
        </p>
        <div className="mt-6 flex flex-wrap items-center gap-x-6 gap-y-2">
          <Link href={LINKS.source}>Source</Link>
          <Link href={LINKS.license}>{LICENSE} licence</Link>
          <Link href={LINKS.releases}>Releases</Link>
        </div>
      </section>

      <Group
        title={label("Updates")}
        hint="Echo asks the GitHub release feed whether a newer signed build exists. It never installs one without asking, and it sends nothing about you — the request carries the version you are on and nothing else."
      >
        <Check
          checked={checkOnStart !== "false"}
          onChange={(v) => setCheckOnStart.mutate(v ? "true" : "false")}
        >
          Check for updates when Echo starts
        </Check>
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <button
            onClick={() => checkUpdate.mutate()}
            disabled={checkUpdate.isPending}
            className="btn-ghost px-3.5 py-[7px] text-[13.5px]"
          >
            {checkUpdate.isPending ? "Checking…" : "Check now"}
          </button>
          {checkUpdate.data && <UpdateResult outcome={checkUpdate.data} />}
        </div>
      </Group>

      <Group
        title={label("Report an issue")}
        hint="What you leave in the box is what gets attached. It says which version, platform and engine you are on — no username, no file paths, no API keys. The buttons open GitHub in your browser; you are the one who presses Submit."
      >
        <textarea
          className="field w-full resize-y font-mono text-[12.5px] leading-relaxed"
          rows={8}
          aria-label="Diagnostics attached to the report"
          value={diagnostics}
          onChange={(e) => setEdited(e.target.value)}
          spellCheck={false}
        />
        {/* The bug form asks what the log said, so this is where the log has
            to be reachable from. It was not reachable anywhere before. */}
        <button onClick={() => void commands.openLog()} className={LINK_CLASS}>
          Show the log file
        </button>
        <div className="flex flex-wrap items-center gap-2.5">
          <button
            onClick={() => void open(issueUrl("bug_report.yml" as IssueKind, { diagnostics }))}
            className="btn-ghost px-3.5 py-[7px] text-[13.5px]"
          >
            Report a bug
          </button>
          <button
            onClick={() => void open(issueUrl("feature_request.yml" as IssueKind, { diagnostics }))}
            className="btn-ghost px-3.5 py-[7px] text-[13.5px]"
          >
            Request a feature
          </button>
          <button
            onClick={() => void copy()}
            className="btn-ghost gap-2 px-3.5 py-[7px] text-[13.5px]"
          >
            {copied ? <CheckIcon className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
            {copied ? "Copied" : "Copy diagnostics"}
          </button>
        </div>
      </Group>

      <Group
        title={label("Contribute")}
        hint="Echo is built by people who wanted it to exist. Bug reports are contributions; so is telling us which part of it reads badly."
      >
        <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
          <Link href={LINKS.contributing}>Contributing guide</Link>
          <Link href={LINKS.discussions}>Discussions</Link>
          <Link href={LINKS.issues}>Open issues</Link>
          <Link href={LINKS.star}>Star Echo</Link>
        </div>
      </Group>
    </>
  );
}
