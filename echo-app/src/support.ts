//! Where Echo's source, its issues and its licence live, and how to reach them.
//!
//! Echo is MIT and public, so "report a bug" is a link, not a support desk.
//! Nothing here has an account, a token or a server behind it: the issue
//! buttons open GitHub's own form with the fields already typed in, and the
//! user is the one who presses Submit, on GitHub, signed in as themselves.

import { openUrl } from "@tauri-apps/plugin-opener";

export const REPO = "vedantnimbarte/Echo";
export const REPO_URL = `https://github.com/${REPO}`;
export const LICENSE = "MIT";

export const LINKS = {
  source: REPO_URL,
  readme: `${REPO_URL}#readme`,
  license: `${REPO_URL}/blob/main/LICENSE`,
  contributing: `${REPO_URL}/blob/main/CONTRIBUTING.md`,
  // Discussions 404s into the repo's own page when the tab is off, which is a
  // dead end; Issues is always there and is where the conversation would go.
  discussions: `${REPO_URL}/discussions`,
  issues: `${REPO_URL}/issues`,
  releases: `${REPO_URL}/releases`,
  star: REPO_URL,
} as const;

/** Which issue form to open. The values are the template filenames. */
export type IssueKind = "bug_report.yml" | "feature_request.yml";

/**
 * GitHub refuses a query string past roughly 8 KB, and answers with a page
 * that has lost what the user typed. Well under it, because the URL also
 * carries the description and percent-encoding triples the size of anything
 * non-ASCII.
 */
const MAX_URL = 6000;

/**
 * The prefilled new-issue URL.
 *
 * `diagnostics` is dropped rather than truncated if the whole thing would be
 * too long: half a diagnostics block invites a wrong conclusion, and a missing
 * one is obvious. The description is the user's own words and is never cut —
 * if that alone is too long, the caller gets a URL GitHub will reject, which
 * is better than silently posting a shortened version of what they wrote.
 */
export function issueUrl(
  kind: IssueKind,
  { title = "", description = "", diagnostics = "" } = {}
): string {
  const build = (diag: string) => {
    const q = new URLSearchParams({ template: kind });
    if (title) q.set("title", title);
    if (description) q.set("description", description);
    if (diag) q.set("diagnostics", diag);
    return `${REPO_URL}/issues/new?${q}`;
  };

  const full = build(diagnostics);
  return full.length <= MAX_URL ? full : build("");
}

/** Open a URL in the user's browser, never inside Echo's own webview. */
export async function open(url: string): Promise<void> {
  await openUrl(url);
}
