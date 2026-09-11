//! What the title bar and the pill claim about where your voice goes.
//!
//! The tag exists to be trusted on one question — does this audio leave the
//! machine — so these assert the sentence it puts in front of the user rather
//! than the markup around it. Every case here is one the old version got
//! wrong: it read the *setting*, and a setting says nothing about whether the
//! provider it names can actually answer.

import { describe, it, expect, beforeEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { vi } from "vitest";

import { settings } from "./setup";
import { EngineTag } from "../components/common/EngineTag";

function mount() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
  return render(
    <QueryClientProvider client={client}>
      <EngineTag onOpen={() => undefined} />
    </QueryClientProvider>
  );
}

/** Fire a backend event at whoever subscribed to it. */
async function emitToListeners(event: string, payload: unknown) {
  const calls = vi.mocked(listen).mock.calls.filter(([name]) => name === event);
  expect(calls.length).toBeGreaterThan(0);
  await act(async () => {
    for (const [, handler] of calls) {
      (handler as (e: unknown) => void)({ payload });
    }
  });
}

describe("the engine tag", () => {
  beforeEach(() => {
    settings.clear();
    vi.mocked(listen).mockClear();
  });

  it("reports the offline engine and its model when nothing is chosen", async () => {
    mount();

    expect(await screen.findByText("Local")).toBeTruthy();
    expect(screen.getByText("base.en")).toBeTruthy();
    expect(
      screen.getByRole("button").getAttribute("aria-label")
    ).toContain("Your audio stays here");
  });

  it("does not claim a cloud provider is receiving audio it cannot receive", async () => {
    settings.set("asr_provider", "openai");
    // The fake catalog ships OpenAI with no key, so nothing reaches it and
    // the tag must not claim otherwise.
    mount();

    expect(await screen.findByText("OpenAI")).toBeTruthy();
    expect(screen.getByText("Needs a key")).toBeTruthy();
    const label = screen.getByRole("button").getAttribute("aria-label") ?? "";
    expect(label).toContain("needs an API key");
    expect(label).toContain("your audio stays here");
  });

  it("names the offline engine after an utterance was diverted to it", async () => {
    settings.set("asr_provider", "openai");
    mount();
    await screen.findByText("OpenAI");

    await emitToListeners("echo://asr-fell-back", { provider: "openai" });

    expect(screen.getByText("OpenAI → Local")).toBeTruthy();
    expect(
      screen.getByRole("button").getAttribute("aria-label")
    ).toContain("did not answer");
  });

  it("stops reporting a diversion once an utterance goes through", async () => {
    settings.set("asr_provider", "openai");
    mount();
    await screen.findByText("OpenAI");

    await emitToListeners("echo://asr-fell-back", { provider: "openai" });
    expect(screen.getByText("OpenAI → Local")).toBeTruthy();

    // The transcript that ends the diverted utterance still reports it…
    await emitToListeners("echo://transcript-final", { text: "hi", language: null });
    expect(screen.getByText("OpenAI → Local")).toBeTruthy();

    // …and the next one, which did not divert, clears it.
    await emitToListeners("echo://transcript-final", { text: "hi", language: null });
    expect(screen.queryByText("OpenAI → Local")).toBeNull();
  });
});
