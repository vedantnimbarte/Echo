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
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { settings, invoked, ANSWERS } from "./setup";
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
    for (const page of ["Settings", "Engine", "Output", "Privacy", "About"]) {
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
  const PAGES: SettingsPage[] = ["settings", "engine", "output", "privacy", "about"];

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

  // Keeping the microphone warm is about opening the audio device, so it has
  // to be there whatever is transcribing. It used to live in Performance,
  // which only renders on the local lane, and vanished for cloud users.
  it("offers the microphone controls whichever engine is running", async () => {
    settings.set("asr_provider", "openai");
    mount(<SettingsPanel page="settings" />);

    expect(await screen.findByText(/Keep the microphone ready/i)).toBeTruthy();
    expect(await screen.findByLabelText("Speech detection")).toBeTruthy();
  });

  // The one screen a user only ever sees after a crash, which is exactly the
  // kind that rots unnoticed. Both halves: silent when there is nothing, and
  // offering the audio back when there is.
  it("says nothing about recovered audio when there is none", async () => {
    mount(<SettingsPanel page="engine" />);
    expect(await screen.findByText(/Choose an audio file/i)).toBeTruthy();
    expect(screen.queryByText(/stopped before it could transcribe/i)).toBeNull();
  });

  it("offers back audio a crash interrupted", async () => {
    ANSWERS.recovered_recordings = ["/data/recovered-1700000000.wav"];
    try {
      const user = userEvent.setup();
      mount(<SettingsPanel page="engine" />);

      expect(await screen.findByText(/stopped before it could transcribe/i)).toBeTruthy();
      await user.click(await screen.findByText("Transcribe it"));

      // Goes through the same decoder an imported file does, on the path the
      // recovery wrote — not on whatever the file picker last returned.
      expect(invoked).toContain("transcribe_file");
    } finally {
      ANSWERS.recovered_recordings = [];
    }
  });

  // The setting only bites when the model is there, so a machine without it
  // must not be shown a choice that does nothing.
  it("says so when the neural detector did not load", async () => {
    ANSWERS.silero_available = false;
    try {
      mount(<SettingsPanel page="settings" />);
      // Awaited first on purpose: the warning renders only once the probe has
      // answered, so reaching it means the picker below has settled too. The
      // picker itself is on screen from the first paint and would be read
      // before the answer arrived.
      expect(await screen.findByText(/didn’t load on this machine/i)).toBeTruthy();

      const picker = screen.getByLabelText("Speech detection") as HTMLSelectElement;
      expect(picker.disabled).toBe(true);
      expect(picker.value).toBe("energy");
    } finally {
      ANSWERS.silero_available = true;
    }
  });

  // About was carved out of the settings page, so the thing worth pinning is
  // that the move happened on both ends — the groups arrived, and they did not
  // stay behind as a second copy.
  it("keeps the open-source groups on About, not on Settings", async () => {
    const { unmount } = mount(<SettingsPanel page="about" />);
    // `findAll`: "Updates" is the group's name and also a word in the sentence
    // on its checkbox, and the assertion is that the group arrived at all.
    for (const group of [/Report an issue/i, /Contribute/i, /Updates/i]) {
      expect((await screen.findAllByText(group)).length).toBeGreaterThan(0);
    }
    // Diagnostics reach the report from the backend, not from a hardcoded string.
    expect(await screen.findByDisplayValue(/OS: windows/)).toBeTruthy();
    unmount();

    mount(<SettingsPanel page="settings" />);
    // Waited for, not asserted on an empty render: the page has to have drawn
    // something before "it is not here" means anything.
    expect(await screen.findByText(/Start Echo when I log in/i)).toBeTruthy();
    expect(screen.queryByText(/Report an issue/i)).toBeNull();
    expect(screen.queryByText(/Contributing guide/i)).toBeNull();
  });

  // The engine page swaps its whole lower half on this choice, and the cloud
  // half must not commit: dictation stays local until a provider with a key is
  // picked, or someone mid-setup is left pointing at a provider that can't
  // answer.
  it("shows cloud providers without switching the engine to one", async () => {
    const user = userEvent.setup();
    mount(<SettingsPanel page="engine" />);

    expect(await screen.findByText("Local models")).toBeTruthy();

    await user.click(await screen.findByText("A cloud provider"));

    expect(await screen.findByText("OpenAI")).toBeTruthy();
    expect(screen.queryByText("Local models")).toBeNull();
    expect(invoked).not.toContain("set_asr_provider");
  });

  it("switches back to the offline engine when local is picked", async () => {
    settings.set("asr_provider", "openai");
    const user = userEvent.setup();
    mount(<SettingsPanel page="engine" />);

    // Opens on the lane that is actually running.
    expect(await screen.findByText("OpenAI")).toBeTruthy();

    await user.click(await screen.findByText("On this machine"));

    expect(await screen.findByText("Local models")).toBeTruthy();
    expect(settings.get("asr_provider")).toBe("local");
  });
});

describe("the other panels", () => {
  it.each([
    ["dictionary", <DictionaryPanel key="d" />],
    ["dictation", <HistoryPanel key="h" onOpenInsights={() => undefined} />],
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
