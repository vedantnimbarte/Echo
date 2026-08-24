// Fill the packaging manifests in from a real release, and emit the checksum
// file the install scripts verify against.
//
//   node scripts/update-manifests.mjs v0.1.0 <dir-of-downloaded-assets>
//
// Run after the release build has attached its assets. Writes:
//   - SHA256SUMS.txt in the asset directory (upload it to the release)
//   - packaging/winget/*.yaml   version + installer URL + SHA256
//   - packaging/homebrew/echo.rb version + SHA256
//   - packaging/snap/snapcraft.yaml  version
//
// Flatpak needs nothing per release: it builds from the local binary and pins
// no version.

import { createHash } from "node:crypto";
import { readdir, readFile, writeFile } from "node:fs/promises";
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

if (winInstaller) {
  const url = `https://github.com/${REPO}/releases/download/${tag}/${winInstaller}`;
  await patch("packaging/winget/Echo.Echo.installer.yaml", [
    [/^PackageVersion: .*$/m, `PackageVersion: ${version}`, "PackageVersion"],
    [/^    InstallerUrl: .*$/m, `    InstallerUrl: ${url}`, "InstallerUrl"],
    [
      /^    InstallerSha256: .*$/m,
      `    InstallerSha256: ${sums.get(winInstaller).toUpperCase()}`,
      "InstallerSha256",
    ],
  ]);
} else {
  console.warn("! no *-setup.exe asset; leaving the winget manifest alone");
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

console.log(`\nmanifests updated for ${tag}`);
