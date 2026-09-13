//! What a check tells the user, and when it stays quiet.
//!
//! The thing worth pinning is that `silent` decides whether an uneventful
//! answer puts a dialog in front of someone who never asked for one — the
//! launch-time check runs on every start.

import { describe, it, expect, beforeEach, vi } from "vitest";
import { message } from "@tauri-apps/plugin-dialog";
import { check } from "@tauri-apps/plugin-updater";

import "./setup";
import { checkForUpdate } from "../update";

describe("checking for an update", () => {
  beforeEach(() => {
    vi.mocked(message).mockClear();
  });

  it("says nothing on a check nobody asked for", async () => {
    // The launch-time check. An answer of "no change" is not worth a dialog.
    expect(await checkForUpdate({ silent: true })).toEqual({ kind: "current", version: "0.3.0" });
    expect(message).not.toHaveBeenCalled();
  });

  it("answers a check someone asked for", async () => {
    // The tray menu, which has nowhere of its own to draw a result.
    expect(await checkForUpdate()).toEqual({ kind: "current", version: "0.3.0" });
    expect(message).toHaveBeenCalledTimes(1);
  });

  it("stays quiet about an unreachable feed unless asked", async () => {
    vi.mocked(check).mockRejectedValueOnce(new Error("offline"));
    expect(await checkForUpdate({ silent: true })).toEqual({ kind: "offline" });
    expect(message).not.toHaveBeenCalled();
  });
});
