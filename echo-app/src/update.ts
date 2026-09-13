import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { ask, message } from "@tauri-apps/plugin-dialog";

/** Settings key for the launch-time check. Absent means on. */
export const CHECK_ON_START = "check_updates_on_start";

/**
 * What a check turned out to be, so a caller with somewhere to draw can say so
 * without re-deriving it. `installing` never returns — the relaunch replaces
 * this process.
 */
export type UpdateOutcome =
  | { kind: "offline" }
  | { kind: "current"; version: string }
  | { kind: "declined"; version: string }
  | { kind: "failed"; message: string };

/**
 * Check GitHub Releases for a newer signed build and, if the user agrees,
 * download + install it and relaunch.
 *
 * `silent` suppresses only the *uneventful* answers — no update, or no feed. A
 * found update always asks, and a failed install always says so, however it
 * was started: those are not things to swallow.
 */
export async function checkForUpdate({ silent = false } = {}): Promise<UpdateOutcome> {
  const say = async (text: string, kind: "info" | "error" = "info") => {
    if (!silent) await message(text, { title: "Update", kind });
  };

  let update;
  try {
    update = await check();
  } catch {
    // Offline, or no release feed.
    await say("Couldn't reach the release feed. Check your connection and try again.");
    return { kind: "offline" };
  }

  if (!update) {
    const version = await currentVersion();
    await say(`Echo ${version} is the latest version.`);
    return { kind: "current", version };
  }

  const wants = await ask(
    `Echo ${update.version} is available (you have ${update.currentVersion}).\n\n` +
      `${update.body ?? ""}\n\nDownload and install now?`,
    { title: "Update available", kind: "info", okLabel: "Install", cancelLabel: "Later" }
  );
  if (!wants) return { kind: "declined", version: update.version };

  try {
    await update.downloadAndInstall();
    await relaunch();
    return { kind: "declined", version: update.version }; // unreachable past relaunch
  } catch (e) {
    // Always loud: a half-installed update is the one outcome the user has to
    // know about, whoever asked for the check.
    await message(`Update failed: ${e}`, { title: "Update", kind: "error" });
    return { kind: "failed", message: String(e) };
  }
}

/** The running version, or `"?"` if the host won't say. */
export async function currentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "?";
  }
}
