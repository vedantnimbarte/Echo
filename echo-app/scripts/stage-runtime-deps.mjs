// Stage the whisper.cpp CLI into src-tauri/resources/bin so the Tauri bundler
// ships it in the installer (letting a fresh install transcribe offline without
// the first-run download). Run this BEFORE `tauri build` in a release job — see
// docs/BUNDLING.md.
//
//   node scripts/stage-runtime-deps.mjs
//
// Only whisper-cli needs staging. The ONNX Runtime that backs Silero VAD is
// statically linked into the executable by `ort`, so there is nothing to ship
// for it.
//
// whisper.cpp publishes a ready-to-run CLI for Windows only; on macOS/Linux the
// app falls back to a `whisper-cli` on PATH, so this script no-ops there. Keep
// WHISPER_TAG / WHISPER_WIN_ASSET in sync with core/asr/binary_manager.rs.

import { createWriteStream } from "node:fs";
import { mkdir, readdir, rename, cp, rm } from "node:fs/promises";
import { pipeline } from "node:stream/promises";
import { Readable } from "node:stream";
import { execFileSync } from "node:child_process";
import path from "node:path";
import os from "node:os";

const ROOT = path.resolve(import.meta.dirname, "..");
const BIN_DIR = path.join(ROOT, "src-tauri", "resources", "bin");

// v1.7.4/v1.7.5 shipped no binary assets; v1.7.6 is the nearest tag that does.
const WHISPER_TAG = "v1.7.6";
const WHISPER_WIN_ASSET = "whisper-bin-x64.zip";

async function download(url, dest) {
  console.log(`↓ ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`GET ${url} → ${res.status}`);
  await pipeline(Readable.fromWeb(res.body), createWriteStream(dest));
}

// The whisper.cpp Windows archive ships ~20 files (bench.exe, stream.exe,
// wchess.exe, SDL2.dll, …). whisper-cli.exe only needs the ggml*/whisper DLLs
// (verified by running it against exactly that set), so keep just the CLI and
// those libraries to keep the installer small.
function keep(name) {
  const l = name.toLowerCase();
  if (l === "whisper-cli.exe" || l === "main.exe") return true;
  return l.endsWith(".dll") && (l.startsWith("ggml") || l.startsWith("whisper"));
}

// Recursively copy the kept files under `src` directly into `dst` (flattened),
// so whisper-cli.exe and its DLLs land side by side.
async function copyFlat(src, dst) {
  for (const entry of await readdir(src, { withFileTypes: true })) {
    const full = path.join(src, entry.name);
    if (entry.isDirectory()) await copyFlat(full, dst);
    else if (keep(entry.name)) await cp(full, path.join(dst, entry.name));
  }
}

async function stageWhisper() {
  await mkdir(BIN_DIR, { recursive: true });
  if (process.platform !== "win32") {
    console.log("• whisper: no prebuilt CLI for this platform; skipping (PATH fallback).");
    return;
  }
  const url = `https://github.com/ggml-org/whisper.cpp/releases/download/${WHISPER_TAG}/${WHISPER_WIN_ASSET}`;
  const zip = path.join(os.tmpdir(), "whisper-cli.zip");
  await download(url, zip);

  const tmp = path.join(os.tmpdir(), "whisper-extract");
  await rm(tmp, { recursive: true, force: true });
  await mkdir(tmp, { recursive: true });
  // Use PowerShell's Expand-Archive: it handles .zip and Windows paths reliably
  // (git-bash's GNU `tar` misreads a `C:\` path as a remote host).
  execFileSync(
    "powershell",
    ["-NoProfile", "-NonInteractive", "-Command", `Expand-Archive -LiteralPath '${zip}' -DestinationPath '${tmp}' -Force`],
    { stdio: "inherit" },
  );
  await copyFlat(tmp, BIN_DIR);

  // Normalise the CLI name: newer archives ship both `whisper-cli.exe` and a
  // legacy `main.exe`. Prefer whisper-cli.exe; rename main.exe only if it is the
  // only one, and drop a redundant main.exe otherwise.
  const files = await readdir(BIN_DIR);
  const mainExe = path.join(BIN_DIR, "main.exe");
  if (files.includes("whisper-cli.exe")) {
    if (files.includes("main.exe")) await rm(mainExe, { force: true });
  } else if (files.includes("main.exe")) {
    await rename(mainExe, path.join(BIN_DIR, "whisper-cli.exe"));
  }
  console.log(`✓ whisper-cli staged into ${BIN_DIR}`);
}

await stageWhisper();
console.log("Done. Now run `tauri build`.");
