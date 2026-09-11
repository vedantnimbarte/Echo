//! Smoke tests for the screens every user meets.
//!
//! Not coverage. The gap these close is that 6,000 lines of frontend had no
//! automated check of any kind, while the Rust side had 240 — so the class of bug
//! that made a panel throw on mount was found by whoever installed the release.
//!
//! Each test asserts the least that would still catch that: the screen renders,
//! and it renders the thing it exists for. Anything more specific becomes a
//! restatement of the markup and has to be rewritten every time the copy changes.

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { settings } from "./setup";
import App from "../App";
import { SettingsPanel, type SettingsPage } from "../components/settings/SettingsPanel";
import { DictionaryPanel } from "../components/dictionary/DictionaryPanel";
import { HistoryPanel } from "../components/history/HistoryPanel";
import { InsightsPanel } from "../components/insights/InsightsPanel";
import { PluginsPanel } from "../components/plugins/PluginsPanel";

/** A fresh client per test: retries off, so a rejected query fails fast here. */
function mount(ui: React.ReactNode) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

describe("the app shell", () => {
  beforeEach(() => {
    settings.set("onboarding_complete", "true");
  });

  it("renders the settings navigation once onboarding is done", async () => {
    mount(<App />);

    // The four pages Settings is split into. If any throws on mount, this fails.
    // `findAllBy` because a page name appears in the nav *and* as a heading once
    // that page is open — the assertion is that the shell rendered them at all.
    for (const page of ["Settings", "Engine", "Output", "Privacy"]) {
      expect((await screen.findAllByText(page)).length).toBeGreaterThan(0);
    }
  });

  it("shows onboarding to someone who has not finished it", async () => {
    settings.delete("onboarding_complete");
    mount(<App />);

    // Onboarding owns the whole window, so the nav must not be there yet.
    await waitFor(() => {
      expect(screen.queryByText("Privacy")).toBeNull();
    });
  });
});

describe("every settings page", () => {
  const PAGES: SettingsPage[] = ["settings", "engine", "output", "privacy"];

  // The original defect was a panel that threw while rendering. A loop over the
  // real page list catches a new page added without being exercised, which a
  // hand-written test per page would not.
  it.each(PAGES)("renders without throwing: %s", async (page) => {
    const { container } = mount(<SettingsPanel page={page} />);
    await waitFor(() => {
      expect(container.textContent?.length ?? 0).toBeGreaterThan(0);
    });
  });

  it("offers the launch-at-login toggle on the settings page", async () => {
    mount(<SettingsPanel page="settings" />);
    expect(await screen.findByText(/Start Echo when I log in/i)).toBeTruthy();
  });

  it("lists the microphone the backend reported", async () => {
    mount(<SettingsPanel page="settings" />);
    expect(await screen.findByText(/Test Microphone/)).toBeTruthy();
  });
});

describe("the other panels", () => {
  it.each([
    ["dictionary", <DictionaryPanel key="d" />],
    ["dictation", <HistoryPanel key="h" />],
    ["insights", <InsightsPanel key="i" />],
    ["plugins", <PluginsPanel key="p" />],
  ])("renders without throwing: %s", async (_name, ui) => {
    const { container } = mount(ui);
    await waitFor(() => {
      expect(container.textContent?.length ?? 0).toBeGreaterThan(0);
    });
  });

  // The charts are the part that can throw on a shape it did not expect — an
  // empty day list, a provider with no rows — so this asserts they drew, not
  // just that the page produced some text.
  it("draws the figures Insights exists for", async () => {
    mount(<InsightsPanel key="insights" />);
    expect(await screen.findByText(/words dictated in total/)).toBeTruthy();
    expect(await screen.findByText("2-day streak")).toBeTruthy();
    expect(await screen.findByText(/On this machine/)).toBeTruthy();
  });
});
