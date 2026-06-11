// In-app updater over signed GitHub Releases (same pattern as Toolzy).

import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type { Update };
export { getVersion };

/** Check GitHub Releases for a newer signed version. Returns null if up to
 *  date or if the check fails (offline, rate-limited) — never throws. */
export async function checkForUpdate(): Promise<Update | null> {
  try {
    return await check();
  } catch {
    return null;
  }
}

/** Download + install the update, then relaunch into the new version. */
export async function installUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}
