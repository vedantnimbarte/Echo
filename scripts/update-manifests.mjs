// Fill the packaging manifests in from a real release, and emit the checksum
// file the install scripts verify against.
//
//   node scripts/update-manifests.mjs v0.1.0 <dir-of-downloaded-assets>
//
// Run after the release build has attached its assets. Writes:
//   - SHA256SUMS.txt in the asset directory (upload it to the release)
//   - packaging/winget/manifests/...  a whole versioned manifest directory
//   - packaging/homebrew/echo.rb version + SHA256
//   - packaging/snap/snapcraft.yaml  version
//
//   - website/src/lib/links.ts   VERSION, which the download links build from
//
// Flatpak needs nothing per release: it builds from the local binary and pins
// no version.

import { createHash } from "node:crypto";
import { copyFile, mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const REPO = "vedantnimbarte/Echo";
const ROOT = path.resolve(import.meta.dirname, "..");

const [, , rawTag, assetDir] = process.argv;
if (!rawTag || !assetDir) {
  console.error(
    "usage: node scripts/update-manifests.mjs <tag> <dir-of-downloaded-assets>"
  );
  process.exit(2);
}

const tag = rawTag.startsWith("v") ? rawTag : `v${rawTag}`;
const version = tag.replace(/^v/, "");

async function sha256(file) {
  const hash = createHash("sha256");
  hash.update(await readFile(file));
  return hash.digest("hex");
}

/** Replace in a file, failing loudly if the pattern didn't match — a silent
 *  no-op here would ship a manifest with a placeholder checksum. */
async function patch(relPath, edits) {
  const file = path.join(ROOT, relPath);
  let text = await readFile(file, "utf8");

  for (const [pattern, replacement, label] of edits) {
    if (!pattern.test(text)) {
      throw new Error(`${relPath}: no match for ${label} (${pattern})`);
    }
    text = text.replace(pattern, replacement);
  }

  await writeFile(file, text);
  console.log(`updated ${relPath}`);
}

// ── Collect assets and checksum them ────────────────────────────────────────

const entries = (await readdir(assetDir, { withFileTypes: true }))
  .filter((e) => e.isFile() && e.name !== "SHA256SUMS.txt")
  .map((e) => e.name)
  .sort();

if (entries.length === 0) {
  throw new Error(`No assets found in ${assetDir}`);
}

const sums = new Map();
for (const name of entries) {
  sums.set(name, await sha256(path.join(assetDir, name)));
}

// Same shape as `sha256sum` output, so `sha256sum -c` works on it directly.
const sumsFile = path.join(assetDir, "SHA256SUMS.txt");
await writeFile(
  sumsFile,
  entries.map((n) => `${sums.get(n)}  ${n}\n`).join("")
);
console.log(`wrote ${sumsFile} (${entries.length} assets)`);

// ── Find the assets the package managers point at ───────────────────────────

const find = (re) => entries.find((n) => re.test(n));

const winInstaller = find(/-setup\.exe$/);
const macDmg = find(/\.dmg$/);

// ── winget ──────────────────────────────────────────────────────────────────
//
// winget wants one directory per version, so a release copies the previous
// version's manifests forward and rewrites the version-specific fields. The
// old directory stays: winget-pkgs keeps every published version.

if (winInstaller) {
  const url = `https://github.com/${REPO}/releases/download/${tag}/${winInstaller}`;
  const wingetRoot = path.join(ROOT, "packaging/winget/manifests/e/Echo/Echo");

  const versions = (await readdir(wingetRoot, { withFileTypes: true }))
    .filter((e) => e.isDirectory())
    .map((e) => e.name)
    .sort();
  const previous = versions[versions.length - 1];
  if (!previous) {
    throw new Error(`No existing winget manifests to copy forward in ${wingetRoot}`);
  }

  const destDir = path.join(wingetRoot, version);
  if (previous !== version) {
    await mkdir(destDir, { recursive: true });
    for (const name of await readdir(path.join(wingetRoot, previous))) {
      await copyFile(
        path.join(wingetRoot, previous, name),
        path.join(destDir, name)
      );
    }
    console.log(`winget: copied ${previous} -> ${version}`);
  }

  const rel = (name) =>
    `packaging/winget/manifests/e/Echo/Echo/${version}/${name}`;

  // PackageVersion appears in all three files and must agree across them.
  for (const name of [
    "Echo.Echo.yaml",
    "Echo.Echo.locale.en-US.yaml",
    "Echo.Echo.installer.yaml",
  ]) {
    await patch(rel(name), [
      [/^PackageVersion: .*$/m, `PackageVersion: ${version}`, "PackageVersion"],
    ]);
  }

  await patch(rel("Echo.Echo.installer.yaml"), [
    [/^  InstallerUrl: .*$/m, `  InstallerUrl: ${url}`, "InstallerUrl"],
    [
      /^  InstallerSha256: .*$/m,
      `  InstallerSha256: ${sums.get(winInstaller).toUpperCase()}`,
      "InstallerSha256",
    ],
  ]);

  await patch(rel("Echo.Echo.locale.en-US.yaml"), [
    [
      /^ReleaseNotesUrl: .*$/m,
      `ReleaseNotesUrl: https://github.com/${REPO}/releases/tag/${tag}`,
      "ReleaseNotesUrl",
    ],
  ]);
} else {
  console.warn("! no *-setup.exe asset; leaving the winget manifests alone");
}

// ── homebrew ────────────────────────────────────────────────────────────────

if (macDmg) {
  // The cask builds its URL from #{version}, so the file name has to match the
  // pattern it expects — warn rather than silently producing a 404 link.
  const expected = `Echo_${version}_aarch64.dmg`;
  if (macDmg !== expected) {
    console.warn(
      `! dmg is named ${macDmg} but the cask URL builds ${expected}; update packaging/homebrew/echo.rb`
    );
  }
  await patch("packaging/homebrew/echo.rb", [
    [/^  version ".*"$/m, `  version "${version}"`, "version"],
    [/^  sha256 ".*"$/m, `  sha256 "${sums.get(macDmg)}"`, "sha256"],
  ]);
} else {
  console.warn("! no .dmg asset; leaving the Homebrew cask alone");
}

// ── snap ────────────────────────────────────────────────────────────────────

await patch("packaging/snap/snapcraft.yaml", [
  [/^version: ".*"$/m, `version: "${version}"`, "version"],
]);

// ── website ─────────────────────────────────────────────────────────────────
//
// The download page builds direct asset URLs from this constant. Left to a
// human it drifts, and the failure mode is every download button 404ing on the
// old version's filenames.

await patch("website/src/lib/links.ts", [
  [
    /^export const VERSION = ".*";$/m,
    `export const VERSION = "${version}";`,
    "VERSION",
  ],
]);

console.log(`\nmanifests updated for ${tag}`);
