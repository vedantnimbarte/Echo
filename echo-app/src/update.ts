import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ask, message } from "@tauri-apps/plugin-dialog";

/**
 * Whether releases are signed and publish a `latest.json` for the updater.
 *
 * Off until release signing is set up — see docs/RELEASING.md, which lists
 * flipping this alongside generating the keypair and re-enabling
 * `bundle.createUpdaterArtifacts`.
 *
 * This is not belt-and-braces for the try/catch below. The updater plugin logs
 * its own ERROR when the endpoint 404s, *before* our catch can swallow it — so
 * with no `latest.json` published, every single launch made a pointless network
 * request and printed a scary line to the log. Not calling it is the only way
 * to stay quiet.
 */
const UPDATER_CONFIGURED = false;

/// Check GitHub Releases for a newer signed build and, if the user agrees,
/// download + install it and relaunch. Silently no-ops when the updater isn't
/// configured or when already up to date, so a missing key never surfaces an
/// error to the user.
export async function checkForUpdate(): Promise<void> {
  if (!UPDATER_CONFIGURED) return;

  let update;
  try {
    update = await check();
  } catch {
    // Offline, or no release feed — nothing to do.
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
