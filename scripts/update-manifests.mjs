// Fill the packaging manifests in from a real release, and emit the checksum
// file the install scripts verify against.
//
//   node scripts/update-manifests.mjs v0.1.0 <dir-of-downloaded-assets>
//
// Run after the release build has attached its assets. Writes:
//   - SHA256SUMS.txt in the asset directory (upload it to the release)
//   - packaging/winget/manifests/...  a whole versioned manifest directory
//   - packaging/homebrew/echo.rb version + SHA256 per macOS architecture
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
// Named by the Tauri bundler, per architecture. A bare /\.dmg$/ would take
// whichever sorts first now that a release can carry two.
const armDmg = find(/_aarch64\.dmg$/);
const intelDmg = find(/_x64\.dmg$/);

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

//
// The cask has two shapes, and which one a release gets depends on whether its
// Intel build succeeded — that build is allowed to fail without blocking the
// release (see `continue-on-error` in release.yml). With both .dmg files it
// takes Homebrew's `arch` form: one URL interpolating #{arch}, a sha256 per
// architecture. With only the arm64 one it stays single-arch and refuses Intel
// outright, rather than pointing Intel Macs at a file that does not exist.
// (`arch` plus `sha256 arm:, intel:` rather than on_arm/on_intel blocks: only
// the file name and its hash differ, which is the case `brew style` wants the
// compact form for.)
//
// The url line is regenerated too, not just the hashes: it carries the arch in
// the file name, and the names are the bundler's, so they are checked here.

if (armDmg) {
  const base = `https://github.com/${REPO}/releases/download/v#{version}/Echo_#{version}_`;
  for (const [name, expected] of [
    [armDmg, `Echo_${version}_aarch64.dmg`],
    [intelDmg, `Echo_${version}_x64.dmg`],
  ]) {
    // Warn, not throw: SHA256SUMS.txt is already written, and failing this
    // step would keep it off the release for every platform.
    if (name && name !== expected) {
      console.warn(`! dmg is named ${name} but the cask URL builds ${expected}; update packaging/homebrew/echo.rb`);
    }
  }

  const header = intelDmg
    ? `  arch arm: "aarch64", intel: "x64"\n\n` +
      `  version "${version}"\n` +
      `  sha256 arm:   "${sums.get(armDmg)}",\n` +
      `         intel: "${sums.get(intelDmg)}"\n\n` +
      `  url "${base}#{arch}.dmg",\n`
    : `  version "${version}"\n` +
      `  sha256 "${sums.get(armDmg)}"\n\n` +
      `  url "${base}aarch64.dmg",\n`;

  const armOnly =
    "  # Apple Silicon only for this version: its release has no Intel .dmg.\n" +
    "  # scripts/update-manifests.mjs drops this line for a release that has one.\n" +
    "  depends_on arch: :arm64\n";

  await patch("packaging/homebrew/echo.rb", [
    [
      /^(?:  arch .*\n\n)?  version ".*"\n  sha256 .*\n(?: +intel: .*\n)?\n  url ".*",\n/m,
      header,
      "version/sha256/url",
    ],
    // Matches with or without an existing arch restriction (and the comment
    // above it), so this both adds and removes it.
    [
      /^(?:  #.*\n)*(?:  depends_on arch: :arm64\n)?(?=  depends_on macos:)/m,
      intelDmg ? "" : armOnly,
      "depends_on",
    ],
  ]);
  if (!intelDmg) {
    console.warn("! no x64 .dmg asset; the Homebrew cask stays Apple Silicon only");
  }
} else {
  console.warn("! no aarch64 .dmg asset; leaving the Homebrew cask alone");
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
