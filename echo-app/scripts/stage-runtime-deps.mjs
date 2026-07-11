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
// Windows uses whisper.cpp's prebuilt CLI (downloaded). macOS/Linux have no
// prebuilt asset, so we build whisper-cli from source here (needs git + cmake +
// a C/C++ compiler on the machine — CI has these). Keep WHISPER_TAG /
// WHISPER_WIN_ASSET in sync with core/asr/binary_manager.rs.

import { createWriteStream } from "node:fs";
import { mkdir, readdir, rename, cp, rm, chmod } from "node:fs/promises";
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

// Build a portable whisper-cli from source (macOS/Linux). Key flags:
//   BUILD_SHARED_LIBS=OFF  → static libwhisper/libggml, so the one binary is
//                            self-contained (no sidecar .so/.dylib to ship).
//   GGML_NATIVE=OFF        → do NOT bake in the CI runner's -march=native CPU
//                            features; otherwise the binary SIGILLs on older
//                            user CPUs. Ship a safe baseline.
//   GGML_OPENMP=OFF        → drop the libgomp runtime dependency (whisper.cpp
//                            still threads via its own pool).
// On macOS we compile a universal (arm64 + x86_64) slice so Intel Macs work.
async function buildWhisperUnix() {
  const src = path.join(os.tmpdir(), "whisper-src");
  const build = path.join(src, "build");
  await rm(src, { recursive: true, force: true });
  console.log(`↓ git clone whisper.cpp ${WHISPER_TAG}`);
  execFileSync(
    "git",
    ["clone", "--depth", "1", "--branch", WHISPER_TAG, "https://github.com/ggml-org/whisper.cpp", src],
    { stdio: "inherit" },
  );

  const cfg = [
    "-B", build,
    "-DCMAKE_BUILD_TYPE=Release",
    "-DBUILD_SHARED_LIBS=OFF",
    "-DGGML_NATIVE=OFF",
    "-DGGML_OPENMP=OFF",
    "-DWHISPER_BUILD_TESTS=OFF",
    "-DWHISPER_BUILD_EXAMPLES=ON",
    "-DWHISPER_BUILD_SERVER=OFF",
  ];
  if (process.platform === "darwin") cfg.push("-DCMAKE_OSX_ARCHITECTURES=arm64;x86_64");
  execFileSync("cmake", cfg, { cwd: src, stdio: "inherit" });
  execFileSync("cmake", ["--build", build, "--config", "Release", "-j", "--target", "whisper-cli"], {
    cwd: src,
    stdio: "inherit",
  });

  // Find the produced binary (usually build/bin/whisper-cli) and stage it.
  const found = await findFile(build, "whisper-cli");
  if (!found) throw new Error(`build succeeded but 'whisper-cli' not found under ${build}`);
  const dest = path.join(BIN_DIR, "whisper-cli");
  await cp(found, dest);
  await chmod(dest, 0o755);
  console.log(`✓ whisper-cli built & staged into ${BIN_DIR}`);
}

async function findFile(dir, name) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      const hit = await findFile(full, name);
      if (hit) return hit;
    } else if (entry.name === name) {
      return full;
    }
  }
  return null;
}

async function stageWhisper() {
  await mkdir(BIN_DIR, { recursive: true });
  if (process.platform !== "win32") {
    await buildWhisperUnix();
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
