//! The prefilled issue URL.
//!
//! This is the one part of the open-source panel with a decision in it: what
//! GitHub receives is whatever survives this function, and the failure modes
//! are quiet ones — a field id that stops matching the template silently drops
//! the prefill, and a URL over the limit loses the lot.

import { describe, it, expect } from "vitest";

import { issueUrl, REPO_URL } from "../support";

/** The template's field ids, which the prefill keys must keep matching. */
const FIELDS = ["title", "description", "diagnostics"];

describe("the prefilled issue URL", () => {
  it("targets the template and fills its fields", () => {
    const url = new URL(
      issueUrl("bug_report.yml", {
        title: "Nothing is typed in Notepad",
        description: "Pressed the hotkey, spoke, nothing appeared.",
        diagnostics: "Echo 0.3.0\nOS: windows (x86_64)",
      })
    );

    expect(url.origin + url.pathname).toBe(`${REPO_URL}/issues/new`);
    expect(url.searchParams.get("template")).toBe("bug_report.yml");
    // Round-trips through percent-encoding, newlines included — a diagnostics
    // block is multi-line and that is exactly what the textarea expects.
    expect(url.searchParams.get("diagnostics")).toBe("Echo 0.3.0\nOS: windows (x86_64)");
    expect(url.searchParams.get("description")).toContain("spoke, nothing appeared");
  });

  it("leaves out fields with nothing in them", () => {
    const url = new URL(issueUrl("feature_request.yml"));
    for (const f of FIELDS) expect(url.searchParams.has(f)).toBe(false);
    expect(url.searchParams.get("template")).toBe("feature_request.yml");
  });

  it("drops the diagnostics rather than let GitHub reject the whole URL", () => {
    // GitHub answers a query string past ~8 KB with a page that has lost
    // everything the user typed, so an over-long report must shed the part
    // nobody wrote by hand.
    const url = new URL(
      issueUrl("bug_report.yml", {
        description: "a".repeat(5000),
        diagnostics: "b".repeat(4000),
      })
    );

    expect(url.searchParams.has("diagnostics")).toBe(false);
    // The user's own words are never the thing that gets cut.
    expect(url.searchParams.get("description")).toBe("a".repeat(5000));
  });

  it("keeps the diagnostics when there is room", () => {
    const url = new URL(
      issueUrl("bug_report.yml", { description: "short", diagnostics: "Echo 0.3.0" })
    );
    expect(url.searchParams.get("diagnostics")).toBe("Echo 0.3.0");
  });
});
