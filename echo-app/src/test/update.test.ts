//! What a check tells the user, and when it stays quiet.
//!
//! Only the unconfigured path is reachable while `UPDATER_CONFIGURED` is false
//! (see docs/RELEASING.md) — but that is the path every shipped build takes
//! today, and the thing worth pinning is that `silent` decides whether it puts
//! a dialog in front of someone who never asked for one.

import { describe, it, expect, beforeEach, vi } from "vitest";
import { message } from "@tauri-apps/plugin-dialog";

import "./setup";
import { checkForUpdate } from "../update";

describe("checking for an update", () => {
  beforeEach(() => {
    vi.mocked(message).mockClear();
  });

  it("says nothing on a check nobody asked for", async () => {
    // The launch-time check. An answer of "no change" is not worth a dialog.
    expect(await checkForUpdate({ silent: true })).toEqual({ kind: "unavailable" });
    expect(message).not.toHaveBeenCalled();
  });

  it("answers a check someone asked for", async () => {
    // The tray menu, which has nowhere of its own to draw a result.
    expect(await checkForUpdate()).toEqual({ kind: "unavailable" });
    expect(message).toHaveBeenCalledTimes(1);
    // Whatever the copy becomes, it has to leave the user somewhere to go.
    expect(String(vi.mocked(message).mock.calls[0][0])).toMatch(/releases page/i);
  });
});
