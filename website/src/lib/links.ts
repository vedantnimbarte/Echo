// Single source of truth for external project links used across the site.
// The canonical repository is derived once so release/doc URLs stay in sync.

export const REPO_URL = "https://github.com/vedantnimbarte/Echo";

const RAW_URL = REPO_URL.replace("github.com", "raw.githubusercontent.com");

/**
 * The release the download page points at.
 *
 * Bumped automatically by `scripts/update-manifests.mjs` during a release, for
 * the same reason the packaging manifests are: hand-maintained, it silently
 * rots into download links that 404 on the previous version's filenames.
 */
export const VERSION = "0.1.0";

const asset = (name: string) =>
  `${REPO_URL}/releases/download/v${VERSION}/${name}`;

/**
 * Direct links to the published build artifacts. These filenames come from the
 * Tauri bundler, so they are verified against the real release when cutting one.
 */
export const DOWNLOADS = {
  macos: asset(`Echo_${VERSION}_aarch64.dmg`),
  windows: asset(`Echo_${VERSION}_x64-setup.exe`),
  windowsMsi: asset(`Echo_${VERSION}_x64_en-US.msi`),
  linuxAppImage: asset(`Echo_${VERSION}_amd64.AppImage`),
  linuxDeb: asset(`Echo_${VERSION}_amd64.deb`),
  linuxRpm: asset(`Echo-${VERSION}-1.x86_64.rpm`),
  checksums: asset("SHA256SUMS.txt"),
} as const;

/** One-liners that fetch the latest release, verify it, and install it. */
export const INSTALL = {
  unix: `curl -fsSL ${RAW_URL}/main/scripts/install.sh | sh`,
  windows: `irm ${RAW_URL}/main/scripts/install.ps1 | iex`,
} as const;

export const LINKS = {
  github: REPO_URL,
  releases: `${REPO_URL}/releases/latest`,
  releaseNotes: `${REPO_URL}/releases/tag/v${VERSION}`,
  contributing: `${REPO_URL}/blob/main/CONTRIBUTING.md`,
  plugins: `${REPO_URL}/blob/main/PLUGINS.md`,
  license: `${REPO_URL}/blob/main/LICENSE`,
  issues: `${REPO_URL}/issues`,
  installing: `${REPO_URL}#installing`,
  wakeWord: `${REPO_URL}/blob/main/docs/WAKE_WORD.md`,
  bundling: `${REPO_URL}/blob/main/docs/BUNDLING.md`,
} as const;
