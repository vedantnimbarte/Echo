import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ask, message } from "@tauri-apps/plugin-dialog";

/// Check GitHub Releases for a newer signed build and, if the user agrees,
/// download + install it and relaunch. Silently no-ops when the updater isn't
/// configured (empty pubkey / no endpoint) or when already up to date, so a
/// missing key never surfaces an error to the user.
export async function checkForUpdate(): Promise<void> {
  let update;
  try {
    update = await check();
  } catch {
    // Not configured yet, offline, or no release feed — nothing to do.
    return;
  }
  if (!update) return;

  const wants = await ask(
    `Echo ${update.version} is available (you have ${update.currentVersion}).\n\n` +
      `${update.body ?? ""}\n\nDownload and install now?`,
    { title: "Update available", kind: "info", okLabel: "Install", cancelLabel: "Later" }
  );
  if (!wants) return;

  try {
    await update.downloadAndInstall();
    await relaunch();
  } catch (e) {
    await message(`Update failed: ${e}`, { title: "Update", kind: "error" });
  }
}
