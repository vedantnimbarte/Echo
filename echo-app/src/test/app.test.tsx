//! Smoke tests for the screens every user meets.
//!
//! Not coverage. The gap these close is that 6,000 lines of frontend had no
//! automated check of any kind, while the Rust side had 240 — so the class of bug
//! that made a panel throw on mount was found by whoever installed the release.
//!
//! Each test asserts the least that would still catch that: the screen renders,
//! and it renders the thing it exists for. Anything more specific becomes a
//! restatement of the markup and has to be rewritten every time the copy changes.

import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";

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

/**
 * Open one of a settings page's sections.
 *
 * Each page shows a single section at a time, so a test that reaches for a
 * control has to say which section it lives in — which is also the assertion
 * that it is still reachable from there at all.
 */
async function openSection(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(await screen.findByRole("button", { name }));
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
    // These are the names on the buttons, which is also what pins the sidebar
    // and each page's own heading to the same words.
    for (const page of ["Settings", "Voice engine", "Output", "Privacy", "About"]) {
      expect((await screen.findAllByText(page)).length).toBeGreaterThan(0);
    }
  });

  // The pill is a separate webview, so its engine tag cannot set this window's
  // page — it asks, and this is the half that has to answer. The ask is only
  // wired in Pill.tsx; what is pinned here is that asking lands somewhere.
  it("goes to the page another window asks for", async () => {
    vi.mocked(listen).mockClear();
    mount(<App />);
    const engine = await screen.findByRole("button", { name: "Voice engine" });
    expect(engine.getAttribute("aria-current")).toBeNull();

    const calls = vi.mocked(listen).mock.calls.filter(([name]) => name === "echo://open-page");
    expect(calls.length).toBeGreaterThan(0);
    await act(async () => {
      for (const [, handler] of calls) {
        (handler as (e: unknown) => void)({ payload: "engine" });
      }
    });

    expect(engine.getAttribute("aria-current")).toBe("page");
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
    const user = userEvent.setup();
    mount(<SettingsPanel page="settings" />);
    await openSection(user, "Microphone");
    expect(await screen.findByText(/Test Microphone/)).toBeTruthy();
  });

  // Keeping the microphone warm is about opening the audio device, so it has
  // to be there whatever is transcribing. It used to live in Performance,
  // which only renders on the local lane, and vanished for cloud users.
  it("offers the microphone controls whichever engine is running", async () => {
    settings.set("asr_provider", "openai");
    const user = userEvent.setup();
    mount(<SettingsPanel page="settings" />);

    await openSection(user, "Microphone");
    expect(await screen.findByText(/Keep the microphone ready/i)).toBeTruthy();
    expect(await screen.findByLabelText("Speech detection")).toBeTruthy();
  });

  // The sections are a way of hiding things, so the two ways back to what is
  // hidden are what have to hold: search reaches across every one of them, and
  // moving to another page does not leave you on a section that page has not
  // got. Both were silent failures waiting to happen.
  it("finds a control from a section that is not open", async () => {
    const user = userEvent.setup();
    mount(<SettingsPanel page="settings" />);

    // Speech detection lives under Microphone; General is what opens.
    expect(screen.queryByLabelText("Speech detection")).toBeNull();
    await user.type(await screen.findByLabelText(/search/i), "silero");

    expect(await screen.findByLabelText("Speech detection")).toBeTruthy();
    // Out of context, so it says where it lives — page, then section.
    expect(await screen.findByText(/Settings · Microphone · Speech detection/)).toBeTruthy();
  });

  it("opens each page on its own first section", async () => {
    const user = userEvent.setup();
    const { rerender } = mount(<SettingsPanel page="settings" />);

    await openSection(user, "Microphone");
    expect(await screen.findByLabelText("Speech detection")).toBeTruthy();

    // Output's sections are its own — the page must not carry the last pick
    // across, and there is no "Microphone" on it to carry it to.
    rerender(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <SettingsPanel page="output" />
      </QueryClientProvider>
    );
    expect(await screen.findByText(/Insert the transcript as soon as/i)).toBeTruthy();
    expect(screen.queryByLabelText(/Insert delay/i)).toBeNull();
  });

  // The clipboard is what "auto" reaches for on any line break or long
  // transcript, so the knob that says how long to hold it has to be offered
  // there too. It used to appear only on an explicit "paste", which hid it from
  // exactly the people whose text was going missing.
  it.each([
    ["paste", true],
    ["auto", true],
    ["type", false],
    // Unset is typing — the Rust side resolves it the same way — so the knob
    // must not appear for someone who has never touched the picker.
    [null, false],
  ])("offers the clipboard hold when the method pastes: %s", async (chosen, offered) => {
    if (chosen === null) settings.delete("injection_method");
    else settings.set("injection_method", chosen);
    const user = userEvent.setup();
    mount(<SettingsPanel page="output" />);

    await openSection(user, "Advanced");
    // Awaited first: Insert delay shares the group and is always there, so
    // reaching it means the group has settled and "not here" means something.
    // Exact labels: each field's hint icon is a button whose accessible name
    // is "About <the same words>", which a regex would match as well.
    expect(await screen.findByLabelText("Insert delay (ms)")).toBeTruthy();
    expect(screen.queryByLabelText("Clipboard hold (ms)") !== null).toBe(offered);
  });

  // The one screen a user only ever sees after a crash, which is exactly the
  // kind that rots unnoticed. Both halves: silent when there is nothing, and
  // offering the audio back when there is.
  it("says nothing about recovered audio when there is none", async () => {
    const user = userEvent.setup();
    mount(<SettingsPanel page="engine" />);
    await openSection(user, "Tools");
    expect(await screen.findByText(/Choose an audio file/i)).toBeTruthy();
    expect(screen.queryByText(/stopped before it could transcribe/i)).toBeNull();
  });

  it("offers back audio a crash interrupted", async () => {
    ANSWERS.recovered_recordings = ["/data/recovered-1700000000.wav"];
    try {
      const user = userEvent.setup();
      mount(<SettingsPanel page="engine" />);

      // No section clicked on purpose: recovered audio is the one thing nobody
      // would think to go looking for on a tab, so the page has to open on it.
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
      const user = userEvent.setup();
      mount(<SettingsPanel page="settings" />);
      await openSection(user, "Microphone");
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

describe("the plugins panel", () => {
  it("keeps the guide out of the way until it is asked for", async () => {
    const user = userEvent.setup();
    mount(<PluginsPanel />);

    // Installed opens, and the install action belongs to it.
    expect(await screen.findByRole("button", { name: /Install from file/i })).toBeTruthy();
    expect(screen.queryByText(/Five steps/i)).toBeNull();

    await openSection(user, "Build one");
    expect(await screen.findByText(/Five steps/i)).toBeTruthy();
    // The guide has nothing to install, so the page's action goes with it.
    // By role, not by text: step 5 names the button in a sentence, and a text
    // query cannot tell the instruction from the thing it points at.
    expect(screen.queryByRole("button", { name: /Install from file/i })).toBeNull();
  });

  // Someone with nothing installed is the person most likely to want to write
  // one, so the empty state is where the offer belongs.
  it("offers the guide from the empty state", async () => {
    const user = userEvent.setup();
    mount(<PluginsPanel />);

    await user.click(await screen.findByRole("button", { name: "write your own" }));
    expect(await screen.findByText(/Five steps/i)).toBeTruthy();
  });

  // The guide once had to warn that capabilities were never called. They are
  // now, and an author needs to know the one line that switches one on.
  it("tells an author how a capability gets called", async () => {
    const user = userEvent.setup();
    mount(<PluginsPanel />);
    await openSection(user, "Build one");

    expect(await screen.findByText(/without that line Echo never calls the trait/i)).toBeTruthy();
    // Each capability, with when it runs.
    for (const capability of ["AudioPlugin", "AsrPlugin", "DictionaryPlugin", "OutputPlugin"]) {
      expect(screen.getByText(new RegExp(`^${capability} ·`))).toBeTruthy();
    }
    expect(screen.queryByText(/will not yet call it/i)).toBeNull();
  });

  // A plugin engine is only useful if someone can pick it, and it is never
  // picked for them: offering an engine is not the same as taking dictation over.
  it("lets an enabled engine plugin be chosen for dictation", async () => {
    ANSWERS.list_plugins = [
      {
        name: "engine",
        version: "0.1.0",
        description: "",
        author: "",
        enabled: true,
        permissions: ["asr"],
      },
    ];
    try {
      const user = userEvent.setup();
      mount(<PluginsPanel />);

      await user.click(await screen.findByRole("button", { name: "Use for dictation" }));
      await waitFor(() => expect(settings.get("asr_provider")).toBe("plugin:engine"));
      expect(await screen.findByText("Transcribing")).toBeTruthy();
    } finally {
      ANSWERS.list_plugins = [];
    }
  });

  it("scaffolds into the folder that was picked, under the name that was typed", async () => {
    const { open } = await import("@tauri-apps/plugin-dialog");
    vi.mocked(open).mockResolvedValueOnce("/home/you/plugins");

    const user = userEvent.setup();
    mount(<PluginsPanel />);
    await openSection(user, "Build one");

    await user.click(await screen.findByRole("button", { name: /Choose a folder/i }));

    await waitFor(() => expect(invoked).toContain("scaffold_plugin"));
    // Where it landed, so the next step is findable rather than guessed at.
    expect(await screen.findByText("/home/you/plugins/my-plugin")).toBeTruthy();
  });

  // The backend validates too — this is the courtesy that stops a round trip,
  // and the message that explains what a legal name is.
  it("will not offer to scaffold a name cargo would refuse", async () => {
    const user = userEvent.setup();
    mount(<PluginsPanel />);
    await openSection(user, "Build one");

    const name = await screen.findByLabelText("Plugin name");
    await user.clear(name);
    await user.type(name, "My Plugin!");

    const button = await screen.findByRole("button", { name: /Choose a folder/i });
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(await screen.findByText(/lowercase letters, digits and hyphens/i)).toBeTruthy();
  });
});

describe("dictionary sync", () => {
  // The warning is asserted, not just the section: it is the sentence someone
  // has to read before pointing Echo at a folder other people can open.
  it("mounts on the dictionary page and says the file is unencrypted", async () => {
    mount(<DictionaryPanel />);
    expect(await screen.findByRole("heading", { name: /Sync/ })).toBeTruthy();
    expect(await screen.findByText(/not encrypted/i)).toBeTruthy();
    expect(await screen.findByText(/Last synced:\s*never/)).toBeTruthy();
    const button = await screen.findByRole("button", { name: /Sync now/ });
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  it("syncs from the button once a folder is chosen and sync is on", async () => {
    settings.set("dictionary_sync_folder", "/home/you/Dropbox");
    settings.set("dictionary_sync_enabled", "true");
    const user = userEvent.setup();
    mount(<DictionaryPanel />);

    const button = await screen.findByRole("button", { name: /Sync now/ });
    await waitFor(() => expect((button as HTMLButtonElement).disabled).toBe(false));
    await user.click(button);
    await waitFor(() => expect(invoked).toContain("sync_dictionary_now"));
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
