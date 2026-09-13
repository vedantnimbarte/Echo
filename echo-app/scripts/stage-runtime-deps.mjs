// Stage the whisper.cpp CLI into src-tauri/resources/bin so the Tauri bundler
// ships it in the installer (letting a fresh install transcribe offline without
// the first-run download). Run this BEFORE `tauri build` in a release job — see
// docs/BUNDLING.md.
//
//   node scripts/stage-runtime-deps.mjs
//
// whisper-cli is staged everywhere. The ONNX Runtime that backs Silero VAD is
// statically linked into the executable by `ort`, so there is nothing to ship
// for it — except on Intel macOS, which has no static build to link and loads
// `libonnxruntime.dylib` from this same directory at runtime instead (see the
// `ort` note in src-tauri/Cargo.toml). Cross-compiling for Intel from an arm64
// Mac, set ECHO_TARGET=x86_64-apple-darwin so the right runtime is staged; the
// release workflow does. `--onnxruntime-only` skips whisper-cli.
//
// Windows uses whisper.cpp's prebuilt CLI (downloaded). macOS/Linux have no
// prebuilt asset, so we build whisper-cli from source here (needs git + cmake +
// a C/C++ compiler on the machine — CI has these). Keep WHISPER_TAG /
// WHISPER_WIN_ASSET in sync with core/asr/binary_manager.rs.

import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { mkdir, readdir, readFile, rename, cp, rm, chmod } from "node:fs/promises";
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
// On macOS we compile a universal (arm64 + x86_64) binary, so the one build
// serves both the arm64 and the Intel .dmg — tauri copies resources/bin as-is,
// whatever `--target` the app itself was compiled for.
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

// Microsoft's last Intel macOS build. 1.24.1 onward publish `osx-arm64` only,
// and `ort` is pinned to API 23 on this target to match (src-tauri/Cargo.toml).
// The digest is the one GitHub records for the release asset
// (`gh release view v1.23.2 --repo microsoft/onnxruntime --json assets`); the
// file goes into an installer, so a changed download must stop the build.
const ORT_X64_VERSION = "1.23.2";
const ORT_X64_ASSET = `onnxruntime-osx-x86_64-${ORT_X64_VERSION}.tgz`;
const ORT_X64_SHA256 = "d10359e16347b57d9959f7e80a225a5b4a66ed7d7e007274a15cae86836485a6";
const ORT_DYLIB = "libonnxruntime.dylib";

async function stageOnnxRuntimeIntel() {
  const url = `https://github.com/microsoft/onnxruntime/releases/download/v${ORT_X64_VERSION}/${ORT_X64_ASSET}`;
  const tgz = path.join(os.tmpdir(), ORT_X64_ASSET);
  await download(url, tgz);
  const actual = createHash("sha256").update(await readFile(tgz)).digest("hex");
  if (actual !== ORT_X64_SHA256) {
    throw new Error(`${ORT_X64_ASSET}: sha256 ${actual}, expected ${ORT_X64_SHA256}`);
  }

  const tmp = path.join(os.tmpdir(), "onnxruntime-extract");
  await rm(tmp, { recursive: true, force: true });
  await mkdir(tmp, { recursive: true });
  // The archive's libonnxruntime.dylib is a symlink to the versioned file, so
  // extract only the real file and stage it under the unversioned name that
  // `load_onnx_runtime` asks for. The rest of the archive is headers and cmake
  // files the app never touches. (The member name really does start "./".)
  const member = `onnxruntime-osx-x86_64-${ORT_X64_VERSION}/lib/libonnxruntime.${ORT_X64_VERSION}.dylib`;
  execFileSync("tar", ["-xzf", tgz, "-C", tmp, `./${member}`], { stdio: "inherit" });
  await cp(path.join(tmp, member), path.join(BIN_DIR, ORT_DYLIB));
  console.log(`✓ ONNX Runtime ${ORT_X64_VERSION} (x86_64) staged into ${BIN_DIR}`);
}

// Which Mac the app is being built for, not which Mac this is: CI builds the
// Intel .dmg on an arm64 runner.
const intelMac = process.env.ECHO_TARGET
  ? process.env.ECHO_TARGET === "x86_64-apple-darwin"
  : process.platform === "darwin" && process.arch === "x64";

// --onnxruntime-only: CI's Intel test job wants the dylib, not a several-minute
// whisper.cpp compile it never runs.
if (!process.argv.includes("--onnxruntime-only")) await stageWhisper();
else await mkdir(BIN_DIR, { recursive: true });
if (intelMac) await stageOnnxRuntimeIntel();
// A dylib left from an earlier Intel staging must not ride along into an
// arm64 or Linux bundle, where nothing loads it.
else await rm(path.join(BIN_DIR, ORT_DYLIB), { force: true });
console.log("Done. Now run `tauri build`.");
